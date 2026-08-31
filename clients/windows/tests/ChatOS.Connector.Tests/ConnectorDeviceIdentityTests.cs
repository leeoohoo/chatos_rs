using System.Text;
using ChatOS.Connector.Connection;
using ChatOS.Connector.Security;
using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Math.EC.Rfc8032;

namespace ChatOS.Connector.Tests;

public sealed class ConnectorDeviceIdentityTests
{
    [Fact]
    public async Task PersistsOneDeviceIdentityAndProducesVerifiableSignature()
    {
        var secrets = new MemorySecretStore();
        var firstProvider = new ConnectorDeviceIdentityProvider(secrets);
        var first = await firstProvider.GetAsync();
        var second = await new ConnectorDeviceIdentityProvider(secrets).GetAsync();
        var payload = Encoding.UTF8.GetBytes("device payload");

        Assert.Equal(first.PublicKey, second.PublicKey);
        Assert.True(Verify(first.PublicKey, payload, first.Sign(payload)));
    }

    [Fact]
    public async Task ReplacesCorruptStoredKeyInsteadOfKeepingBrokenIdentity()
    {
        var secrets = new MemorySecretStore
        {
            Values = { ["device-signing-key-v1"] = "not-base64" },
        };

        var identity = await new ConnectorDeviceIdentityProvider(secrets).GetAsync();

        Assert.StartsWith("ed25519:", identity.PublicKey);
        Assert.NotEqual("not-base64", secrets.Values["device-signing-key-v1"]);
    }

    [Fact]
    public async Task SocketRequestUsesWindowsDeviceIdentityAndExactProtocolPayload()
    {
        var now = new DateTimeOffset(2026, 8, 30, 12, 0, 0, TimeSpan.Zero);
        var identityProvider = new ConnectorDeviceIdentityProvider(new MemorySecretStore());
        var factory = new ConnectorSocketRequestFactory(
            identityProvider,
            new FixedTimeProvider(now),
            () => "12345678-1234-1234-1234-1234567890ab",
            () => "S-1-5-21-100-200-300-400");

        var request = await factory.CreateAsync(
            new Uri("https://gateway.example/base"),
            "token-1",
            "device/1");

        Assert.Equal(
            "wss://gateway.example/api/local-connectors/devices/device%2F1/connect",
            request.Uri.AbsoluteUri);
        Assert.Equal("Bearer token-1", request.Headers["Authorization"]);
        Assert.Equal("ed25519", request.Headers["x-local-connector-device-signature-alg"]);
        Assert.Equal("v2", request.Headers["x-local-connector-device-signature-version"]);
        Assert.Equal(
            "S-1-5-21-100-200-300-400",
            request.Headers["x-local-connector-windows-user-sid"]);

        var identity = await identityProvider.GetAsync();
        var timestamp = request.Headers["x-local-connector-device-timestamp"];
        var nonce = request.Headers["x-local-connector-device-nonce"];
        var payload = Encoding.UTF8.GetBytes(ConnectorDeviceIdentity.ConnectionPayloadV2(
            "device/1",
            timestamp,
            nonce,
            "/api/local-connectors/devices/device%2F1/connect",
            "S-1-5-21-100-200-300-400"));
        Assert.True(Verify(
            identity.PublicKey,
            payload,
            request.Headers["x-local-connector-device-signature"]));
    }

    [Fact]
    public void HeartbeatDisconnectsAfterThreeConsecutiveMissesAndPongResetsCounter()
    {
        var monitor = new ConnectorHeartbeatMonitor();
        var connectedAt = DateTimeOffset.UtcNow;
        monitor.Reset(connectedAt);

        Assert.False(monitor.CompleteHeartbeat(connectedAt.AddSeconds(1)));
        Assert.False(monitor.CompleteHeartbeat(connectedAt.AddSeconds(2)));
        monitor.RecordPong(connectedAt.AddSeconds(3));
        Assert.False(monitor.CompleteHeartbeat(connectedAt.AddSeconds(3)));
        Assert.Equal(0, monitor.MissedAcknowledgements);
        Assert.False(monitor.CompleteHeartbeat(connectedAt.AddSeconds(4)));
        Assert.False(monitor.CompleteHeartbeat(connectedAt.AddSeconds(5)));
        Assert.True(monitor.CompleteHeartbeat(connectedAt.AddSeconds(6)));
    }

    private static bool Verify(string publicKey, byte[] payload, string signature)
    {
        var key = new Ed25519PublicKeyParameters(Decode(publicKey["ed25519:".Length..]));
        return key.Verify(Ed25519.Algorithm.Ed25519, null, payload, Decode(signature));
    }

    private static byte[] Decode(string value)
    {
        var normalized = value.Replace('-', '+').Replace('_', '/');
        normalized += new string('=', (4 - normalized.Length % 4) % 4);
        return Convert.FromBase64String(normalized);
    }

    private sealed class MemorySecretStore : IConnectorSecretStore
    {
        public Dictionary<string, string> Values { get; } = new(StringComparer.Ordinal);

        public ValueTask<string?> GetAsync(
            string key,
            CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(Values.GetValueOrDefault(key));

        public ValueTask SetAsync(
            string key,
            string value,
            CancellationToken cancellationToken = default)
        {
            Values[key] = value;
            return ValueTask.CompletedTask;
        }

        public ValueTask DeleteAsync(
            string key,
            CancellationToken cancellationToken = default)
        {
            Values.Remove(key);
            return ValueTask.CompletedTask;
        }
    }

    private sealed class FixedTimeProvider(DateTimeOffset value) : TimeProvider
    {
        public override DateTimeOffset GetUtcNow() => value;
    }
}
