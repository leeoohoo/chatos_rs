using System.Buffers.Binary;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace ChatOS.NetworkGuard.Contracts;

public static class NetworkGuardProtocol
{
    public const int MajorVersion = 1;
    public const int MinorVersion = 0;
    public const int MaximumFrameBytes = 256 * 1024;
    public const string PipeName = "ChatOS.NetworkGuard.v1";
}

public enum NetworkGuardOperation
{
    Health,
    AcquireLease,
    RenewLease,
    ReleaseLease,
}

public sealed record NetworkGuardRequest(
    [property: JsonPropertyName("protocol_major")] int ProtocolMajor,
    [property: JsonPropertyName("protocol_minor")] int ProtocolMinor,
    [property: JsonPropertyName("correlation_id")] string CorrelationId,
    [property: JsonPropertyName("operation")] NetworkGuardOperation Operation,
    [property: JsonPropertyName("policy")] ControlledNetworkPolicyEnvelope? Policy = null,
    [property: JsonPropertyName("appcontainer_sid")] string? AppContainerSid = null,
    [property: JsonPropertyName("process_id")] int? ProcessId = null,
    [property: JsonPropertyName("lease_id")] string? LeaseId = null);

public sealed record NetworkGuardResponse(
    [property: JsonPropertyName("protocol_major")] int ProtocolMajor,
    [property: JsonPropertyName("protocol_minor")] int ProtocolMinor,
    [property: JsonPropertyName("correlation_id")] string CorrelationId,
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("failure_code")] string? FailureCode = null,
    [property: JsonPropertyName("service_version")] string? ServiceVersion = null,
    [property: JsonPropertyName("driver_version")] string? DriverVersion = null,
    [property: JsonPropertyName("driver_ready")] bool DriverReady = false,
    [property: JsonPropertyName("self_test_passed")] bool SelfTestPassed = false,
    [property: JsonPropertyName("lease_id")] string? LeaseId = null,
    [property: JsonPropertyName("lease_expires_at")] DateTimeOffset? LeaseExpiresAt = null,
    [property: JsonPropertyName("active_lease_count")] int ActiveLeaseCount = 0);

public static class NetworkGuardProtocolCodec
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.SnakeCaseLower) },
    };

    public static async Task WriteAsync<T>(
        Stream stream,
        T value,
        CancellationToken cancellationToken = default)
    {
        var payload = JsonSerializer.SerializeToUtf8Bytes(value, JsonOptions);
        if (payload.Length is < 1 or > NetworkGuardProtocol.MaximumFrameBytes)
        {
            throw new InvalidOperationException("NetworkGuard protocol frame size is invalid.");
        }

        var header = new byte[sizeof(int)];
        BinaryPrimitives.WriteInt32BigEndian(header, payload.Length);
        await stream.WriteAsync(header, cancellationToken).ConfigureAwait(false);
        await stream.WriteAsync(payload, cancellationToken).ConfigureAwait(false);
        await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
    }

    public static async Task<T> ReadAsync<T>(
        Stream stream,
        CancellationToken cancellationToken = default)
    {
        var header = new byte[sizeof(int)];
        await ReadExactlyAsync(stream, header, cancellationToken).ConfigureAwait(false);
        var length = BinaryPrimitives.ReadInt32BigEndian(header);
        if (length is < 1 or > NetworkGuardProtocol.MaximumFrameBytes)
        {
            throw new InvalidDataException("NetworkGuard protocol frame size is invalid.");
        }

        var payload = new byte[length];
        await ReadExactlyAsync(stream, payload, cancellationToken).ConfigureAwait(false);
        try
        {
            return JsonSerializer.Deserialize<T>(payload, JsonOptions)
                ?? throw new InvalidDataException("NetworkGuard protocol frame is empty.");
        }
        catch (JsonException exception)
        {
            throw new InvalidDataException("NetworkGuard protocol frame contains invalid JSON.", exception);
        }
    }

    private static async Task ReadExactlyAsync(
        Stream stream,
        Memory<byte> destination,
        CancellationToken cancellationToken)
    {
        var offset = 0;
        while (offset < destination.Length)
        {
            var count = await stream.ReadAsync(destination[offset..], cancellationToken)
                .ConfigureAwait(false);
            if (count == 0)
            {
                throw new EndOfStreamException("NetworkGuard protocol frame ended early.");
            }
            offset += count;
        }
    }
}
