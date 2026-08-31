using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Relay;

internal static class RelayRequestSignature
{
    public static byte[] Payload(RelayRequest request)
    {
        var keyId = Required(request.PlatformSignatureKeyId, "platform_signature_key_id");
        var algorithm = Required(request.PlatformSignatureAlgorithm, "platform_signature_alg");
        var timestamp = request.PlatformTimestamp
            ?? throw new RelayRequestException(403, "Relay platform_timestamp is required.");
        var nonce = Required(request.PlatformNonce, "platform_nonce");
        var headers = JsonSerializer.SerializeToElement(
            request.Headers.ToDictionary(pair => pair.Key, pair => pair.Value));
        var value = string.Join('\n',
        [
            "v1",
            request.Type,
            request.RequestId,
            request.OwnerUserId ?? string.Empty,
            request.DeviceId ?? string.Empty,
            request.WorkspaceId,
            request.Method ?? string.Empty,
            request.Path ?? string.Empty,
            keyId,
            algorithm,
            timestamp.ToString(System.Globalization.CultureInfo.InvariantCulture),
            nonce,
            CanonicalJson.Serialize(headers),
            CanonicalJson.Serialize(request.Body),
        ]);
        return Encoding.UTF8.GetBytes(value);
    }

    private static string Required(string? value, string field) =>
        !string.IsNullOrWhiteSpace(value)
            ? value.Trim()
            : throw new RelayRequestException(403, $"Relay {field} is required.");
}
