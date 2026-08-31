using System.Text;
using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Math.EC.Rfc8032;
using Org.BouncyCastle.Security;

namespace ChatOS.Connector.Security;

public sealed class ConnectorDeviceIdentity
{
    private readonly Ed25519PrivateKeyParameters _privateKey;

    internal ConnectorDeviceIdentity(Ed25519PrivateKeyParameters privateKey)
    {
        _privateKey = privateKey;
    }

    public string PublicKey =>
        "ed25519:" + Base64Url(_privateKey.GeneratePublicKey().GetEncoded());

    public string Sign(ReadOnlySpan<byte> payload)
    {
        var signature = new byte[Ed25519PrivateKeyParameters.SignatureSize];
        _privateKey.Sign(Ed25519.Algorithm.Ed25519, null, payload, signature);
        return Base64Url(signature);
    }

    public string SignConnection(
        string deviceId,
        string timestamp,
        string nonce,
        string path) =>
        Sign(Encoding.UTF8.GetBytes(ConnectionPayload(deviceId, timestamp, nonce, path)));

    public string SignConnectionV2(
        string deviceId,
        string timestamp,
        string nonce,
        string path,
        string windowsUserSid) =>
        Sign(Encoding.UTF8.GetBytes(ConnectionPayloadV2(
            deviceId,
            timestamp,
            nonce,
            path,
            windowsUserSid)));

    public static string ConnectionPayload(
        string deviceId,
        string timestamp,
        string nonce,
        string path) =>
        $"v1\n{deviceId}\n{timestamp}\n{nonce}\n{path}";

    public static string ConnectionPayloadV2(
        string deviceId,
        string timestamp,
        string nonce,
        string path,
        string windowsUserSid) =>
        $"v2\n{deviceId}\n{timestamp}\n{nonce}\n{path}\n{windowsUserSid}";

    private static string Base64Url(byte[] value) =>
        Convert.ToBase64String(value).TrimEnd('=').Replace('+', '-').Replace('/', '_');
}

public sealed class ConnectorDeviceIdentityProvider
{
    private const string SecretKey = "device-signing-key-v1";
    private readonly SemaphoreSlim _loadGate = new(1, 1);
    private readonly IConnectorSecretStore _secrets;
    private ConnectorDeviceIdentity? _cached;

    public ConnectorDeviceIdentityProvider(IConnectorSecretStore secrets)
    {
        _secrets = secrets;
    }

    public async ValueTask<ConnectorDeviceIdentity> GetAsync(
        CancellationToken cancellationToken = default)
    {
        if (_cached is not null)
        {
            return _cached;
        }

        await _loadGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_cached is not null)
            {
                return _cached;
            }

            var stored = await _secrets.GetAsync(SecretKey, cancellationToken).ConfigureAwait(false);
            if (!string.IsNullOrWhiteSpace(stored))
            {
                try
                {
                    var privateKey = new Ed25519PrivateKeyParameters(Convert.FromBase64String(stored));
                    _cached = new ConnectorDeviceIdentity(privateKey);
                    return _cached;
                }
                catch (FormatException)
                {
                    await _secrets.DeleteAsync(SecretKey, cancellationToken).ConfigureAwait(false);
                }
                catch (ArgumentException)
                {
                    await _secrets.DeleteAsync(SecretKey, cancellationToken).ConfigureAwait(false);
                }
            }

            var generated = new Ed25519PrivateKeyParameters(new SecureRandom());
            await _secrets.SetAsync(
                SecretKey,
                Convert.ToBase64String(generated.GetEncoded()),
                cancellationToken).ConfigureAwait(false);
            _cached = new ConnectorDeviceIdentity(generated);
            return _cached;
        }
        finally
        {
            _loadGate.Release();
        }
    }
}
