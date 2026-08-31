using ChatOS.Connector.Security;
using System.Security.Principal;

namespace ChatOS.Connector.Connection;

public sealed record ConnectorSocketRequest(
    Uri Uri,
    IReadOnlyDictionary<string, string> Headers);

public sealed class ConnectorSocketRequestFactory
{
    private readonly ConnectorDeviceIdentityProvider _identityProvider;
    private readonly TimeProvider _timeProvider;
    private readonly Func<string> _nonceFactory;
    private readonly Func<string?> _windowsSidFactory;

    public ConnectorSocketRequestFactory(
        ConnectorDeviceIdentityProvider identityProvider,
        TimeProvider? timeProvider = null,
        Func<string>? nonceFactory = null,
        Func<string?>? windowsSidFactory = null)
    {
        _identityProvider = identityProvider;
        _timeProvider = timeProvider ?? TimeProvider.System;
        _nonceFactory = nonceFactory ?? (() => Guid.NewGuid().ToString());
        _windowsSidFactory = windowsSidFactory ?? CurrentWindowsUserSid;
    }

    public async Task<ConnectorSocketRequest> CreateAsync(
        Uri gatewayBaseUri,
        string accessToken,
        string deviceId,
        CancellationToken cancellationToken = default)
    {
        if (!gatewayBaseUri.IsAbsoluteUri || gatewayBaseUri.Scheme is not ("http" or "https"))
        {
            throw new ArgumentException("Connector gateway must be an absolute HTTP(S) URL.", nameof(gatewayBaseUri));
        }

        if (string.IsNullOrWhiteSpace(accessToken))
        {
            throw new ArgumentException("Connector access token is required.", nameof(accessToken));
        }

        if (string.IsNullOrWhiteSpace(deviceId))
        {
            throw new ArgumentException("Connector device id is required.", nameof(deviceId));
        }

        var encodedDeviceId = Uri.EscapeDataString(deviceId.Trim());
        var path = $"/api/local-connectors/devices/{encodedDeviceId}/connect";
        var uriBuilder = new UriBuilder(gatewayBaseUri)
        {
            Scheme = gatewayBaseUri.Scheme == Uri.UriSchemeHttps ? "wss" : "ws",
            Port = gatewayBaseUri.IsDefaultPort ? -1 : gatewayBaseUri.Port,
            Path = path,
            Query = string.Empty,
        };
        var timestamp = _timeProvider.GetUtcNow().ToUnixTimeSeconds().ToString(
            System.Globalization.CultureInfo.InvariantCulture);
        var nonce = _nonceFactory();
        if (nonce.Length is < 16 or > 128)
        {
            throw new InvalidOperationException("Connector nonce must contain 16 to 128 characters.");
        }

        var identity = await _identityProvider.GetAsync(cancellationToken).ConfigureAwait(false);
        var windowsUserSid = _windowsSidFactory()?.Trim();
        var headers = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["Authorization"] = $"Bearer {accessToken.Trim()}",
            ["x-local-connector-device-id"] = deviceId.Trim(),
            ["x-local-connector-device-timestamp"] = timestamp,
            ["x-local-connector-device-nonce"] = nonce,
            ["x-local-connector-device-signature-alg"] = "ed25519",
        };
        if (!string.IsNullOrWhiteSpace(windowsUserSid))
        {
            headers["x-local-connector-device-signature-version"] = "v2";
            headers["x-local-connector-windows-user-sid"] = windowsUserSid;
            headers["x-local-connector-device-signature"] = identity.SignConnectionV2(
                deviceId.Trim(),
                timestamp,
                nonce,
                path,
                windowsUserSid);
        }
        else
        {
            headers["x-local-connector-device-signature-version"] = "v1";
            headers["x-local-connector-device-signature"] =
                identity.SignConnection(deviceId.Trim(), timestamp, nonce, path);
        }
        return new ConnectorSocketRequest(uriBuilder.Uri, headers);
    }

    private static string? CurrentWindowsUserSid()
    {
        if (!OperatingSystem.IsWindows())
        {
            return null;
        }

        using var identity = WindowsIdentity.GetCurrent();
        return identity.User?.Value;
    }
}
