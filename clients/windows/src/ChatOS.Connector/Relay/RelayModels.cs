using System.Text.Json;
using System.Text.Json.Serialization;

namespace ChatOS.Connector.Relay;

public sealed record RelayRequest
{
    [JsonPropertyName("type")]
    public required string Type { get; init; }

    [JsonPropertyName("request_id")]
    public required string RequestId { get; init; }

    [JsonPropertyName("owner_user_id")]
    public string? OwnerUserId { get; init; }

    [JsonPropertyName("device_id")]
    public string? DeviceId { get; init; }

    [JsonPropertyName("workspace_id")]
    public required string WorkspaceId { get; init; }

    [JsonPropertyName("method")]
    public string? Method { get; init; }

    [JsonPropertyName("path")]
    public string? Path { get; init; }

    [JsonPropertyName("headers")]
    public IReadOnlyDictionary<string, string> Headers { get; init; } =
        new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);

    [JsonPropertyName("body")]
    public JsonElement Body { get; init; }

    [JsonPropertyName("platform_signature")]
    public string? PlatformSignature { get; init; }

    [JsonPropertyName("platform_signature_key_id")]
    public string? PlatformSignatureKeyId { get; init; }

    [JsonPropertyName("platform_signature_alg")]
    public string? PlatformSignatureAlgorithm { get; init; }

    [JsonPropertyName("platform_timestamp")]
    public long? PlatformTimestamp { get; init; }

    [JsonPropertyName("platform_nonce")]
    public string? PlatformNonce { get; init; }

    public string? Header(string name) =>
        Headers.FirstOrDefault(pair =>
            string.Equals(pair.Key, name, StringComparison.OrdinalIgnoreCase)).Value?.Trim() is { Length: > 0 } value
                ? value
                : null;
}

public sealed record RelayResponse
{
    [JsonPropertyName("type")]
    public required string Type { get; init; }

    [JsonPropertyName("request_id")]
    public required string RequestId { get; init; }

    [JsonPropertyName("status")]
    public required int Status { get; init; }

    [JsonPropertyName("headers")]
    public IReadOnlyDictionary<string, string> Headers { get; init; } =
        new Dictionary<string, string>();

    [JsonPropertyName("body")]
    public required JsonElement Body { get; init; }
}

public sealed class RelayRequestException : Exception
{
    public RelayRequestException(int statusCode, string message)
        : base(message)
    {
        StatusCode = statusCode;
    }

    public int StatusCode { get; }
}

public interface IRelayRequestHandler
{
    bool CanHandle(string requestType);

    string ResponseType(string requestType);

    Task<RelayHandlerResult> HandleAsync(RelayRequest request, CancellationToken cancellationToken);
}

public sealed record RelayHandlerResult(int Status, JsonElement Body)
{
    public static RelayHandlerResult Ok(JsonElement body) => new(200, body);
}

public interface IRelayRequestVerifier
{
    Task VerifyAsync(RelayRequest request, CancellationToken cancellationToken);
}

public interface IRelayOneWayHandler
{
    bool CanHandle(string requestType);

    Task HandleAsync(RelayRequest request, CancellationToken cancellationToken);
}
