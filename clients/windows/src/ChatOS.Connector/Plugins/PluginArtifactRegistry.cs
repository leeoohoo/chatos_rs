using System.Collections.Concurrent;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;

namespace ChatOS.Connector.Plugins;

public sealed record PluginArtifactOwner(
    [property: JsonPropertyName("owner_user_id")] string OwnerUserId,
    [property: JsonPropertyName("run_id")] string RunId,
    [property: JsonPropertyName("device_id")] string DeviceId,
    [property: JsonPropertyName("workspace_id")] string WorkspaceId,
    [property: JsonPropertyName("plugin_id")] string PluginId,
    [property: JsonPropertyName("release_id")] string ReleaseId,
    [property: JsonPropertyName("artifact_sha256")] string PackageArtifactSha256,
    [property: JsonPropertyName("component_key")] string ComponentKey,
    [property: JsonPropertyName("adapter_session_id")] string AdapterSessionId);

public sealed record PluginArtifactDescriptor(
    [property: JsonPropertyName("artifact_id")] string ArtifactId,
    [property: JsonPropertyName("owner")] PluginArtifactOwner Owner,
    [property: JsonPropertyName("workspace_relative_path")] string WorkspaceRelativePath,
    [property: JsonPropertyName("display_name")] string DisplayName,
    [property: JsonPropertyName("media_type")] string MediaType,
    [property: JsonPropertyName("size_bytes")] long SizeBytes,
    [property: JsonPropertyName("sha256")] string Sha256,
    [property: JsonPropertyName("created_at")] DateTimeOffset CreatedAt,
    [property: JsonPropertyName("producer_tool_name")] string ProducerToolName,
    [property: JsonPropertyName("downloadable")] bool Downloadable,
    [property: JsonPropertyName("mutable")] bool Mutable);

public interface IPluginArtifactService
{
    Task<IReadOnlyList<PluginArtifactDescriptor>> ListAsync(
        string? adapterSessionId = null,
        CancellationToken cancellationToken = default);

    Task<PluginArtifactDescriptor> CopyToAsync(
        string artifactId,
        Stream destination,
        CancellationToken cancellationToken = default);
}

internal sealed class PluginArtifactRegistry : IPluginArtifactService
{
    internal const long MaximumArtifactBytes = 64L * 1024 * 1024;
    private const int MaximumArtifactsPerCall = 64;
    private const int MaximumRegisteredArtifacts = 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private readonly ConcurrentDictionary<string, RegisteredArtifact> _artifacts =
        new(StringComparer.Ordinal);

    public Task<IReadOnlyList<PluginArtifactDescriptor>> ListAsync(
        string? adapterSessionId = null,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        RemoveMissing();
        IReadOnlyList<PluginArtifactDescriptor> values = _artifacts.Values
            .Where(value => adapterSessionId is null ||
                string.Equals(
                    value.Descriptor.Owner.AdapterSessionId,
                    adapterSessionId,
                    StringComparison.Ordinal))
            .OrderByDescending(value => value.Descriptor.CreatedAt)
            .ThenBy(value => value.Descriptor.ArtifactId, StringComparer.Ordinal)
            .Select(value => value.Descriptor)
            .ToArray();
        return Task.FromResult(values);
    }

    public async Task<PluginArtifactDescriptor> CopyToAsync(
        string artifactId,
        Stream destination,
        CancellationToken cancellationToken = default)
    {
        if (!destination.CanWrite)
        {
            throw new ArgumentException("Plugin artifact destination must be writable.", nameof(destination));
        }

        var registered = _artifacts.TryGetValue(artifactId, out var value)
            ? value
            : throw new PluginRuntimeException("Plugin Artifact does not exist.");
        await ValidateRegisteredAsync(registered, cancellationToken).ConfigureAwait(false);
        await using var input = new FileStream(
            registered.AbsolutePath,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read,
            64 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        await input.CopyToAsync(destination, 64 * 1024, cancellationToken).ConfigureAwait(false);
        return registered.Descriptor;
    }

    internal async Task<JsonElement> RegisterAsync(
        PluginRuntimeIdentity identity,
        string ownerUserId,
        string deviceId,
        string artifactDirectory,
        string toolName,
        JsonElement result,
        CancellationToken cancellationToken)
    {
        if (result.ValueKind != JsonValueKind.Object ||
            !result.TryGetProperty("_meta", out var meta) ||
            meta.ValueKind != JsonValueKind.Object ||
            !meta.TryGetProperty("chatos/artifacts", out var candidatesValue))
        {
            return result;
        }

        if (candidatesValue.ValueKind != JsonValueKind.Array)
        {
            throw new PluginRuntimeException("Plugin MCP Artifact candidates must be an array.");
        }

        var candidates = candidatesValue.EnumerateArray().ToArray();
        if (candidates.Length > MaximumArtifactsPerCall)
        {
            throw new PluginRuntimeException("Plugin MCP returned too many Artifact candidates.");
        }

        if (string.IsNullOrWhiteSpace(artifactDirectory))
        {
            throw new PluginRuntimeException("Plugin MCP transport does not provide an Artifact directory.");
        }

        var root = ResolveArtifactRoot(artifactDirectory);
        var prepared = new List<(string ProducerId, RegisteredArtifact Artifact)>(candidates.Length);
        var producerIds = new HashSet<string>(StringComparer.Ordinal);
        foreach (var value in candidates)
        {
            cancellationToken.ThrowIfCancellationRequested();
            McpArtifactCandidate candidate;
            try
            {
                candidate = value.Deserialize<McpArtifactCandidate>(JsonOptions)
                    ?? throw new JsonException("Artifact candidate is empty.");
            }
            catch (JsonException exception)
            {
                throw new PluginRuntimeException("Plugin MCP Artifact candidate is invalid.", exception);
            }

            ValidateCandidate(candidate);
            if (!producerIds.Add(candidate.ProducerArtifactId))
            {
                throw new PluginRuntimeException("Plugin MCP Artifact producer ids must be unique per call.");
            }

            var path = ResolveArtifactCandidate(root, candidate.RelativePath);
            var (size, hash) = await HashAsync(path, cancellationToken).ConfigureAwait(false);
            if (size != candidate.SizeBytes ||
                !string.Equals(hash, candidate.Sha256, StringComparison.Ordinal))
            {
                throw new PluginRuntimeException(
                    "Plugin MCP Artifact size or SHA-256 does not match its candidate descriptor.");
            }

            var mediaType = MediaTypeForPath(path);
            if (!string.Equals(mediaType, candidate.MediaType, StringComparison.Ordinal))
            {
                throw new PluginRuntimeException(
                    "Plugin MCP Artifact MIME type does not match its file extension.");
            }

            var artifactId = $"pa_{Guid.NewGuid():N}";
            var descriptor = new PluginArtifactDescriptor(
                artifactId,
                new PluginArtifactOwner(
                    ownerUserId,
                    identity.RunId,
                    deviceId,
                    identity.WorkspaceId ?? string.Empty,
                    identity.PluginId,
                    identity.ReleaseId,
                    identity.ArtifactSha256,
                    identity.ComponentKey,
                    identity.AdapterSessionId),
                $"chatos-plugin-artifacts/{identity.AdapterSessionId}/{artifactId}/{candidate.DisplayName}",
                candidate.DisplayName,
                candidate.MediaType,
                size,
                hash,
                DateTimeOffset.UtcNow,
                toolName,
                Downloadable: true,
                Mutable: false);
            prepared.Add((candidate.ProducerArtifactId, new RegisteredArtifact(descriptor, path)));
        }

        foreach (var value in prepared)
        {
            _artifacts[value.Artifact.Descriptor.ArtifactId] = value.Artifact;
        }

        PruneCapacity();
        var rootNode = JsonNode.Parse(result.GetRawText())?.AsObject()
            ?? throw new PluginRuntimeException("Plugin MCP result could not be materialized.");
        var metaNode = rootNode["_meta"]?.AsObject()
            ?? throw new PluginRuntimeException("Plugin MCP result metadata is invalid.");
        metaNode["chatos/artifacts"] = new JsonArray(prepared.Select(value =>
            JsonSerializer.SerializeToNode(new
            {
                producer_artifact_id = value.ProducerId,
                artifact = value.Artifact.Descriptor,
            }, JsonOptions)).ToArray());
        return JsonSerializer.SerializeToElement(rootNode, JsonOptions);
    }

    private static string ResolveArtifactRoot(string path)
    {
        var root = Path.GetFullPath(path);
        if (!Directory.Exists(root) || IsReparsePoint(root))
        {
            throw new PluginRuntimeException("Plugin MCP Artifact directory is unavailable or unsafe.");
        }

        return Path.TrimEndingDirectorySeparator(root);
    }

    private static string ResolveArtifactCandidate(string root, string relativePath)
    {
        if (Path.IsPathFullyQualified(relativePath) || relativePath.Length > 4096)
        {
            throw new PluginRuntimeException("Plugin MCP Artifact path is not a safe relative path.");
        }

        var normalized = relativePath.Replace('\\', '/');
        var parts = normalized.Split('/', StringSplitOptions.None);
        if (parts.Length == 0 || parts.Any(part =>
                part.Length == 0 || part is "." or ".." || part.Contains(':') || part.Contains('\0')))
        {
            throw new PluginRuntimeException("Plugin MCP Artifact path is invalid.");
        }

        var cursor = root;
        foreach (var part in parts)
        {
            cursor = Path.Combine(cursor, part);
            if (!File.Exists(cursor) && !Directory.Exists(cursor) || IsReparsePoint(cursor))
            {
                throw new PluginRuntimeException("Plugin MCP Artifact path is missing or contains a reparse point.");
            }
        }

        var candidate = Path.GetFullPath(cursor);
        var comparison = OperatingSystem.IsWindows()
            ? StringComparison.OrdinalIgnoreCase
            : StringComparison.Ordinal;
        var rootPrefix = root + Path.DirectorySeparatorChar;
        if (!candidate.StartsWith(rootPrefix, comparison) || !File.Exists(candidate))
        {
            throw new PluginRuntimeException(
                "Plugin MCP Artifact escaped its session directory or is not a regular file.");
        }

        return candidate;
    }

    private static async Task<(long Size, string Sha256)> HashAsync(
        string path,
        CancellationToken cancellationToken)
    {
        var before = new FileInfo(path);
        if (!before.Exists || before.Length > MaximumArtifactBytes || IsReparsePoint(path))
        {
            throw new PluginRuntimeException(
                "Plugin MCP Artifact is not a regular file or exceeds the 64 MB limit.");
        }

        await using var stream = new FileStream(
            path,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read,
            64 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var buffer = new byte[64 * 1024];
        long total = 0;
        while (true)
        {
            var read = await stream.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (read == 0)
            {
                break;
            }

            total += read;
            if (total > MaximumArtifactBytes)
            {
                throw new PluginRuntimeException("Plugin MCP Artifact exceeded the 64 MB limit while hashing.");
            }

            hash.AppendData(buffer, 0, read);
        }

        var after = new FileInfo(path);
        if (total != before.Length || after.Length != before.Length ||
            after.LastWriteTimeUtc != before.LastWriteTimeUtc)
        {
            throw new PluginRuntimeException("Plugin MCP Artifact changed while registering.");
        }

        return (total, Convert.ToHexString(hash.GetHashAndReset()).ToLowerInvariant());
    }

    private static async Task ValidateRegisteredAsync(
        RegisteredArtifact registered,
        CancellationToken cancellationToken)
    {
        var (size, hash) = await HashAsync(registered.AbsolutePath, cancellationToken).ConfigureAwait(false);
        if (size != registered.Descriptor.SizeBytes ||
            !string.Equals(hash, registered.Descriptor.Sha256, StringComparison.Ordinal) ||
            !string.Equals(
                MediaTypeForPath(registered.AbsolutePath),
                registered.Descriptor.MediaType,
                StringComparison.Ordinal))
        {
            throw new PluginRuntimeException("Plugin Artifact changed after registration.");
        }
    }

    private static void ValidateCandidate(McpArtifactCandidate candidate)
    {
        if (candidate.ProducerArtifactId.Trim() != candidate.ProducerArtifactId ||
            candidate.ProducerArtifactId.Length is < 1 or > 256 ||
            candidate.DisplayName.Trim() != candidate.DisplayName ||
            candidate.DisplayName.Length is < 1 or > 512 ||
            candidate.DisplayName.IndexOfAny(['/', '\\']) >= 0 ||
            candidate.MediaType.Trim() != candidate.MediaType ||
            candidate.MediaType.Length is < 1 or > 256 ||
            candidate.SizeBytes is < 0 or > MaximumArtifactBytes ||
            candidate.Sha256.Length != 64 ||
            candidate.Sha256.Any(value => !(char.IsAsciiDigit(value) || value is >= 'a' and <= 'f')))
        {
            throw new PluginRuntimeException("Plugin MCP Artifact candidate identity is invalid.");
        }
    }

    private static string? MediaTypeForPath(string path) => Path.GetExtension(path).ToLowerInvariant() switch
    {
        ".png" => "image/png",
        ".jpg" or ".jpeg" => "image/jpeg",
        ".pdf" => "application/pdf",
        ".json" or ".har" => "application/json",
        ".txt" => "text/plain",
        ".csv" => "text/csv",
        ".zip" => "application/zip",
        ".docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ".xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ".pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => null,
    };

    private void RemoveMissing()
    {
        foreach (var pair in _artifacts)
        {
            if (!File.Exists(pair.Value.AbsolutePath))
            {
                _artifacts.TryRemove(pair.Key, out _);
            }
        }
    }

    private void PruneCapacity()
    {
        if (_artifacts.Count <= MaximumRegisteredArtifacts)
        {
            return;
        }

        foreach (var value in _artifacts.Values
                     .OrderBy(item => item.Descriptor.CreatedAt)
                     .ThenBy(item => item.Descriptor.ArtifactId, StringComparer.Ordinal)
                     .Take(_artifacts.Count - MaximumRegisteredArtifacts))
        {
            _artifacts.TryRemove(value.Descriptor.ArtifactId, out _);
        }
    }

    private static bool IsReparsePoint(string path) =>
        (File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0;

    private sealed record RegisteredArtifact(PluginArtifactDescriptor Descriptor, string AbsolutePath);

    private sealed record McpArtifactCandidate
    {
        [JsonPropertyName("producer_artifact_id")]
        public required string ProducerArtifactId { get; init; }
        [JsonPropertyName("relative_path")]
        public required string RelativePath { get; init; }
        [JsonPropertyName("display_name")]
        public required string DisplayName { get; init; }
        [JsonPropertyName("media_type")]
        public required string MediaType { get; init; }
        [JsonPropertyName("size_bytes")]
        public required long SizeBytes { get; init; }
        [JsonPropertyName("sha256")]
        public required string Sha256 { get; init; }
    }
}
