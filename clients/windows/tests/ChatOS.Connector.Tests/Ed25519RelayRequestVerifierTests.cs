using System.Text.Json;
using ChatOS.Connector.Relay;
using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Math.EC.Rfc8032;
using Org.BouncyCastle.Security;

namespace ChatOS.Connector.Tests;

public sealed class Ed25519RelayRequestVerifierTests
{
    private static readonly DateTimeOffset Now =
        new(2026, 8, 30, 12, 0, 0, TimeSpan.Zero);

    [Fact]
    public void CanonicalJsonMatchesRelayProtocolOrdering()
    {
        using var document = JsonDocument.Parse("""
            {"z":1,"a":{"d":[3,{"y":false,"b":true}],"b":"中文/x"}}
            """);

        var canonical = CanonicalJson.Serialize(document.RootElement);

        Assert.Equal(
            "{\"a\":{\"b\":\"中文/x\",\"d\":[3,{\"b\":true,\"y\":false}]},\"z\":1}",
            canonical);
    }

    [Fact]
    public async Task AcceptsValidSignatureAndRejectsReplay()
    {
        var privateKey = new Ed25519PrivateKeyParameters(new SecureRandom());
        var request = SignedRequest(privateKey);
        var verifier = Verifier(privateKey);

        await verifier.VerifyAsync(request, CancellationToken.None);
        var replay = await Assert.ThrowsAsync<RelayRequestException>(() =>
            verifier.VerifyAsync(request, CancellationToken.None));

        Assert.Equal(403, replay.StatusCode);
        Assert.Contains("already used", replay.Message);
    }

    [Fact]
    public async Task TamperedBodyDoesNotConsumeNonce()
    {
        var privateKey = new Ed25519PrivateKeyParameters(new SecureRandom());
        var original = SignedRequest(privateKey);
        var tampered = original with
        {
            Body = JsonSerializer.SerializeToElement(new { tool = "browser", args = new { a = 1, b = 3 } }),
        };
        var verifier = Verifier(privateKey);

        var error = await Assert.ThrowsAsync<RelayRequestException>(() =>
            verifier.VerifyAsync(tampered, CancellationToken.None));
        Assert.Contains("verification failed", error.Message);

        await verifier.VerifyAsync(original, CancellationToken.None);
    }

    [Fact]
    public async Task ChecksOwnerAndDeviceEvenWhenUnsignedMessagesAreAllowed()
    {
        var context = new RelaySecurityContext(
            "owner-1",
            "device-1",
            new RemoteControlTrust(false, 300, new Dictionary<string, string>()));
        var verifier = new Ed25519RelayRequestVerifier(
            new StubSecurityContextProvider(context),
            new FixedTimeProvider(Now));
        var request = BaseRequest() with { OwnerUserId = "other-owner" };

        var error = await Assert.ThrowsAsync<RelayRequestException>(() =>
            verifier.VerifyAsync(request, CancellationToken.None));

        Assert.Equal(403, error.StatusCode);
        Assert.Contains("owner", error.Message);
    }

    [Fact]
    public async Task RejectsExpiredSignature()
    {
        var privateKey = new Ed25519PrivateKeyParameters(new SecureRandom());
        var request = SignedRequest(privateKey, timestamp: Now.AddMinutes(-10).ToUnixTimeSeconds());
        var verifier = Verifier(privateKey);

        var error = await Assert.ThrowsAsync<RelayRequestException>(() =>
            verifier.VerifyAsync(request, CancellationToken.None));

        Assert.Contains("expired", error.Message);
    }

    private static Ed25519RelayRequestVerifier Verifier(Ed25519PrivateKeyParameters privateKey)
    {
        var publicKey = "ed25519:" + Base64Url(privateKey.GeneratePublicKey().GetEncoded());
        var context = new RelaySecurityContext(
            "owner-1",
            "device-1",
            new RemoteControlTrust(
                true,
                300,
                new Dictionary<string, string> { ["relay-key-1"] = publicKey }));
        return new Ed25519RelayRequestVerifier(
            new StubSecurityContextProvider(context),
            new FixedTimeProvider(Now));
    }

    private static RelayRequest SignedRequest(
        Ed25519PrivateKeyParameters privateKey,
        long? timestamp = null)
    {
        var request = BaseRequest() with
        {
            PlatformSignatureKeyId = "relay-key-1",
            PlatformSignatureAlgorithm = "ed25519",
            PlatformTimestamp = timestamp ?? Now.ToUnixTimeSeconds(),
            PlatformNonce = "12345678-1234-1234-1234-1234567890ab",
        };
        var payload = RelayRequestSignature.Payload(request);
        var signature = new byte[Ed25519PrivateKeyParameters.SignatureSize];
        privateKey.Sign(Ed25519.Algorithm.Ed25519, null, payload, signature);
        return request with { PlatformSignature = Base64Url(signature) };
    }

    private static RelayRequest BaseRequest() => new()
    {
        Type = "plugin_execute_request",
        RequestId = "request-1",
        OwnerUserId = "owner-1",
        DeviceId = "device-1",
        WorkspaceId = "workspace-1",
        Method = "POST",
        Path = "/plugins/execute",
        Headers = new Dictionary<string, string> { ["x-demo"] = "1" },
        Body = JsonSerializer.SerializeToElement(new { tool = "browser", args = new { b = 2, a = 1 } }),
    };

    private static string Base64Url(byte[] value) =>
        Convert.ToBase64String(value).TrimEnd('=').Replace('+', '-').Replace('/', '_');

    private sealed class StubSecurityContextProvider(RelaySecurityContext context)
        : IRelaySecurityContextProvider
    {
        public Task<RelaySecurityContext> GetAsync(CancellationToken cancellationToken) =>
            Task.FromResult(context);
    }

    private sealed class FixedTimeProvider(DateTimeOffset value) : TimeProvider
    {
        public override DateTimeOffset GetUtcNow() => value;
    }
}
