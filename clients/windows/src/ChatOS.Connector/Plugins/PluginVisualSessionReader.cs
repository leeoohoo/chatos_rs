using System.Text.Json;
using System.Text.Json.Serialization;

namespace ChatOS.Connector.Plugins;

public sealed record PluginVisualSessionOwner(
    string ConversationId,
    string? TurnId = null,
    string? SourceUserMessageId = null,
    string? TaskId = null,
    string? TaskRunId = null,
    string? TaskTitle = null);

public sealed record PluginVisualSession(
    string Id,
    string AdapterSessionId,
    string PluginId,
    string ComponentKey,
    string PluginDisplayName,
    string Title,
    string? TargetApplication,
    ulong FrameSequence,
    DateTimeOffset CapturedAt,
    byte[]? FrameData,
    string MimeType,
    int? Width,
    int? Height,
    PluginVisualSessionOwner Owner);

public interface IPluginVisualSessionService
{
    Task<IReadOnlyList<PluginVisualSession>> ReadAsync(
        IReadOnlySet<string>? loadFrameDataForAdapterSessionIds = null,
        CancellationToken cancellationToken = default);
}

internal sealed class PluginVisualSessionReader(
    PluginRuntimeSessionStore sessions,
    TimeProvider? timeProvider = null) : IPluginVisualSessionService
{
    private const long MaximumMetadataBytes = 16 * 1024;
    private const long MaximumFrameBytes = 2 * 1024 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    public async Task<IReadOnlyList<PluginVisualSession>> ReadAsync(
        IReadOnlySet<string>? loadFrameDataForAdapterSessionIds = null,
        CancellationToken cancellationToken = default)
    {
        var result = new List<(PluginVisualSession Session, DateTimeOffset BoundAt)>();
        foreach (var descriptor in sessions.VisualDescriptors())
        {
            cancellationToken.ThrowIfCancellationRequested();
            var session = await ReadAsync(
                descriptor,
                loadFrameDataForAdapterSessionIds?.Contains(descriptor.Identity.AdapterSessionId) ?? true,
                cancellationToken).ConfigureAwait(false);
            if (session is not null)
            {
                result.Add((session, descriptor.OwnerBoundAt));
            }
        }

        return result.OrderByDescending(value => value.BoundAt)
            .ThenBy(value => value.Session.AdapterSessionId, StringComparer.Ordinal)
            .Select(value => value.Session)
            .ToArray();
    }

    private async Task<PluginVisualSession?> ReadAsync(
        PluginRuntimeSessionStore.PluginVisualDescriptor descriptor,
        bool loadFrameData,
        CancellationToken cancellationToken)
    {
        try
        {
            var root = Path.GetFullPath(descriptor.VisualSessionDirectory);
            if (!Directory.Exists(root) || IsReparsePoint(root))
            {
                return null;
            }

            var host = await ReadBoundedJsonAsync<VisualHostMetadata>(
                Path.Combine(root, "host.json"), cancellationToken).ConfigureAwait(false);
            if (host is null || host.ProtocolVersion != 1 ||
                !string.Equals(
                    host.AdapterSessionId,
                    descriptor.Identity.AdapterSessionId,
                    StringComparison.Ordinal) ||
                !string.Equals(host.PluginId, descriptor.Identity.PluginId, StringComparison.Ordinal) ||
                !string.Equals(host.ComponentKey, descriptor.Identity.ComponentKey, StringComparison.Ordinal))
            {
                return null;
            }

            var metadata = await ReadBoundedJsonAsync<VisualSessionMetadata>(
                Path.Combine(root, "session.json"), cancellationToken).ConfigureAwait(false);
            if (metadata is null || metadata.ProtocolVersion != 1 || metadata.Status != "running" ||
                !SafeLabel(metadata.SessionId, 256) || !SafeLabel(metadata.Title, 120) ||
                metadata.TargetApplication is not null && !SafeLabel(metadata.TargetApplication, 120) ||
                metadata.CapturedAt > _timeProvider.GetUtcNow().AddSeconds(5) ||
                metadata.Width is <= 0 or > 32768 || metadata.Height is <= 0 or > 32768)
            {
                return null;
            }

            var expectedFile = metadata.MimeType switch
            {
                "image/jpeg" => "frame.jpg",
                "image/png" => "frame.png",
                _ => null,
            };
            if (expectedFile is null || !string.Equals(metadata.FrameFile, expectedFile, StringComparison.Ordinal))
            {
                return null;
            }

            var framePath = Path.Combine(root, expectedFile);
            var frameInfo = new FileInfo(framePath);
            if (!frameInfo.Exists || frameInfo.Length is <= 0 or > MaximumFrameBytes ||
                IsReparsePoint(framePath))
            {
                return null;
            }

            byte[]? frame = null;
            if (loadFrameData)
            {
                await using var stream = new FileStream(
                    framePath,
                    FileMode.Open,
                    FileAccess.Read,
                    FileShare.ReadWrite | FileShare.Delete,
                    64 * 1024,
                    FileOptions.Asynchronous | FileOptions.SequentialScan);
                if (stream.Length is <= 0 or > MaximumFrameBytes)
                {
                    return null;
                }

                using var buffer = new MemoryStream(checked((int)stream.Length));
                await stream.CopyToAsync(buffer, 64 * 1024, cancellationToken).ConfigureAwait(false);
                if (buffer.Length != frameInfo.Length)
                {
                    return null;
                }

                frame = buffer.ToArray();
            }

            return new PluginVisualSession(
                metadata.SessionId,
                descriptor.Identity.AdapterSessionId,
                descriptor.Identity.PluginId,
                descriptor.Identity.ComponentKey,
                descriptor.DisplayName,
                metadata.Title,
                metadata.TargetApplication,
                metadata.FrameSequence,
                metadata.CapturedAt,
                frame,
                metadata.MimeType,
                metadata.Width,
                metadata.Height,
                descriptor.Owner);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or JsonException)
        {
            return null;
        }
    }

    private static async Task<T?> ReadBoundedJsonAsync<T>(
        string path,
        CancellationToken cancellationToken)
    {
        var info = new FileInfo(path);
        if (!info.Exists || info.Length is <= 0 or > MaximumMetadataBytes || IsReparsePoint(path))
        {
            return default;
        }

        await using var stream = new FileStream(
            path,
            FileMode.Open,
            FileAccess.Read,
            FileShare.ReadWrite | FileShare.Delete,
            16 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        return await JsonSerializer.DeserializeAsync<T>(stream, JsonOptions, cancellationToken)
            .ConfigureAwait(false);
    }

    private static bool SafeLabel(string value, int maximumBytes) =>
        !string.IsNullOrWhiteSpace(value) &&
        System.Text.Encoding.UTF8.GetByteCount(value) <= maximumBytes &&
        !value.Any(char.IsControl);

    private static bool IsReparsePoint(string path) =>
        (File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0;

    private sealed record VisualHostMetadata
    {
        [JsonPropertyName("protocol_version")]
        public required int ProtocolVersion { get; init; }
        [JsonPropertyName("adapter_session_id")]
        public required string AdapterSessionId { get; init; }
        [JsonPropertyName("plugin_id")]
        public required string PluginId { get; init; }
        [JsonPropertyName("component_key")]
        public required string ComponentKey { get; init; }
    }

    private sealed record VisualSessionMetadata
    {
        [JsonPropertyName("protocol_version")]
        public required int ProtocolVersion { get; init; }
        [JsonPropertyName("session_id")]
        public required string SessionId { get; init; }
        [JsonPropertyName("status")]
        public required string Status { get; init; }
        [JsonPropertyName("title")]
        public required string Title { get; init; }
        [JsonPropertyName("target_app")]
        public string? TargetApplication { get; init; }
        [JsonPropertyName("mime_type")]
        public required string MimeType { get; init; }
        [JsonPropertyName("frame_file")]
        public required string FrameFile { get; init; }
        [JsonPropertyName("frame_sequence")]
        public ulong FrameSequence { get; init; }
        [JsonPropertyName("captured_at")]
        public required DateTimeOffset CapturedAt { get; init; }
        [JsonPropertyName("width")]
        public int? Width { get; init; }
        [JsonPropertyName("height")]
        public int? Height { get; init; }
    }
}
