using System.Collections.Concurrent;
using System.Security.Cryptography;
using System.Text;
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
        string? workspaceRoot,
        IReadOnlySet<string> permissionSnapshot,
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

        if (!permissionSnapshot.Contains("artifact.create"))
        {
            throw new PluginRuntimeException(
                "Plugin is not allowed to register local artifacts.");
        }
        if (string.IsNullOrWhiteSpace(identity.WorkspaceId) ||
            string.IsNullOrWhiteSpace(workspaceRoot))
        {
            throw new PluginRuntimeException(
                "Plugin project artifacts require a project workspace.");
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
        var projectRoot = ResolveProjectRoot(workspaceRoot);
        var preparedCandidates = new List<(string ProducerId, McpArtifactCandidate Candidate,
            string SourcePath, long Size, string Sha256)>(candidates.Length);
        var producerIds = new HashSet<string>(StringComparer.Ordinal);
        foreach (var value in candidates)
        {
            cancellationToken.ThrowIfCancellationRequested();
            McpArtifactCandidate candidate;
            try
            {
                candidate = ParseCandidate(value);
            }
            catch (Exception exception) when (exception is JsonException or InvalidOperationException)
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
            if (candidate.SizeBytes is { } declaredSize && size != declaredSize ||
                candidate.Sha256 is { } declaredHash &&
                !string.Equals(hash, declaredHash, StringComparison.Ordinal))
            {
                throw new PluginRuntimeException(
                    "Plugin MCP Artifact size or SHA-256 does not match its candidate descriptor.");
            }

            if (!MediaTypesForPath(path).Contains(candidate.MediaType, StringComparer.Ordinal))
            {
                throw new PluginRuntimeException(
                    "Plugin MCP Artifact MIME type does not match its file extension.");
            }

            preparedCandidates.Add((candidate.ProducerArtifactId, candidate, path, size, hash));
        }

        var prepared = new List<(string ProducerId, RegisteredArtifact Artifact)>(candidates.Length);
        var createdDirectories = new List<string>();
        try
        {
            foreach (var value in preparedCandidates)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var artifactId = $"pa_{Guid.NewGuid():N}";
                var relativePath = string.Join('/', "chatos-plugin-artifacts",
                    SafePathComponent(identity.AdapterSessionId), artifactId,
                    value.Candidate.DisplayName);
                var parent = CreateManagedDirectory(projectRoot,
                    ["chatos-plugin-artifacts", SafePathComponent(identity.AdapterSessionId), artifactId]);
                createdDirectories.Add(parent);
                var destination = Path.Combine(parent, value.Candidate.DisplayName);
                await CopyAndValidateAsync(
                    value.SourcePath, destination, value.Size, value.Sha256, cancellationToken)
                    .ConfigureAwait(false);
                var descriptor = new PluginArtifactDescriptor(
                    artifactId,
                    new PluginArtifactOwner(
                        ownerUserId,
                        identity.RunId,
                        deviceId,
                        identity.WorkspaceId,
                        identity.PluginId,
                        identity.ReleaseId,
                        identity.ArtifactSha256,
                        identity.ComponentKey,
                        identity.AdapterSessionId),
                    relativePath,
                    value.Candidate.DisplayName,
                    value.Candidate.MediaType,
                    value.Size,
                    value.Sha256,
                    DateTimeOffset.UtcNow,
                    toolName,
                    Downloadable: true,
                    Mutable: false);
                prepared.Add((value.ProducerId, new RegisteredArtifact(descriptor, destination)));
            }
        }
        catch
        {
            foreach (var directory in createdDirectories.AsEnumerable().Reverse())
            {
                TryDeleteDirectory(directory);
            }
            throw;
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
        if (prepared.Count != 0)
        {
            var deliveries = prepared.Select(value => new
            {
                display_name = value.Artifact.Descriptor.DisplayName,
                workspace_relative_path = value.Artifact.Descriptor.WorkspaceRelativePath,
                markdown = MarkdownLink(value.Artifact.Descriptor),
            }).ToArray();
            var deliveryText = "ChatOS managed delivery links follow. In the final response, use the exact " +
                "Markdown target below; do not replace it with the producer relativePath, a basename, " +
                "an absolute path, or a file:// URL.\n" +
                string.Join('\n', deliveries.Select(value => $"- {value.markdown}"));
            var content = rootNode["content"] as JsonArray ?? [];
            content.Insert(0, JsonSerializer.SerializeToNode(new
            {
                type = "text",
                text = deliveryText,
            }, JsonOptions));
            rootNode["content"] = content;
            if (rootNode["structuredContent"] is JsonObject structured)
            {
                structured["chatos_managed_delivery"] = JsonSerializer.SerializeToNode(new
                {
                    instruction = "Use one of these exact Markdown values in the final response. " +
                        "These are Host-registered downloadable project artifacts.",
                    links = deliveries,
                }, JsonOptions);
            }
        }
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

    private static string ResolveProjectRoot(string path)
    {
        var root = Path.GetFullPath(path);
        if (!Directory.Exists(root) || IsReparsePoint(root))
        {
            throw new PluginRuntimeException("Plugin project workspace is unavailable or unsafe.");
        }

        return Path.TrimEndingDirectorySeparator(root);
    }

    private static string CreateManagedDirectory(string root, IReadOnlyList<string> components)
    {
        var cursor = root;
        foreach (var component in components)
        {
            cursor = Path.Combine(cursor, component);
            if (File.Exists(cursor) || Directory.Exists(cursor))
            {
                if (!Directory.Exists(cursor) || IsReparsePoint(cursor))
                {
                    throw new PluginRuntimeException(
                        "Plugin artifact project directory is unsafe.");
                }
            }
            else
            {
                Directory.CreateDirectory(cursor);
            }
        }

        return cursor;
    }

    private static async Task CopyAndValidateAsync(
        string source,
        string destination,
        long expectedSize,
        string expectedSha256,
        CancellationToken cancellationToken)
    {
        await using var input = new FileStream(
            source,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read,
            64 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        await using (var output = new FileStream(
            destination,
            FileMode.CreateNew,
            FileAccess.Write,
            FileShare.None,
            64 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan))
        {
            await input.CopyToAsync(output, 64 * 1024, cancellationToken).ConfigureAwait(false);
            await output.FlushAsync(cancellationToken).ConfigureAwait(false);
        }

        var (size, sha256) = await HashAsync(destination, cancellationToken).ConfigureAwait(false);
        if (size != expectedSize || !string.Equals(sha256, expectedSha256, StringComparison.Ordinal))
        {
            throw new PluginRuntimeException("Plugin artifact changed while being persisted.");
        }
    }

    private static string SafePathComponent(string value)
    {
        var result = new string(value.Where(character =>
            char.IsAsciiLetterOrDigit(character) || character is '-' or '_').ToArray());
        return result.Length != 0
            ? result
            : throw new PluginRuntimeException("Plugin artifact session identity is invalid.");
    }

    private static string MarkdownLink(PluginArtifactDescriptor descriptor) =>
        $"[{MarkdownLabel(descriptor.DisplayName)}]({MarkdownTarget(descriptor.WorkspaceRelativePath)})";

    private static string MarkdownLabel(string value) => value
        .Replace("\\", "\\\\", StringComparison.Ordinal)
        .Replace("[", "\\[", StringComparison.Ordinal)
        .Replace("]", "\\]", StringComparison.Ordinal);

    private static string MarkdownTarget(string value) => string.Join('/',
        value.Split('/').Select(Uri.EscapeDataString));

    private static void TryDeleteDirectory(string path)
    {
        try
        {
            if (Directory.Exists(path)) Directory.Delete(path, recursive: true);
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
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
            !MediaTypesForPath(registered.AbsolutePath)
                .Contains(registered.Descriptor.MediaType, StringComparer.Ordinal))
        {
            throw new PluginRuntimeException("Plugin Artifact changed after registration.");
        }
    }

    private static void ValidateCandidate(McpArtifactCandidate candidate)
    {
        if (candidate.ProducerArtifactId.Trim() != candidate.ProducerArtifactId ||
            Encoding.UTF8.GetByteCount(candidate.ProducerArtifactId) is < 1 or > 256 ||
            candidate.ProducerArtifactId.Any(char.IsControl) ||
            candidate.DisplayName.Trim() != candidate.DisplayName ||
            Encoding.UTF8.GetByteCount(candidate.DisplayName) is < 1 or > 512 ||
            candidate.DisplayName.IndexOfAny(['/', '\\']) >= 0 ||
            candidate.DisplayName.Any(char.IsControl) ||
            candidate.MediaType.Trim() != candidate.MediaType ||
            Encoding.UTF8.GetByteCount(candidate.MediaType) is < 1 or > 256 ||
            candidate.MediaType.Any(char.IsControl) ||
            candidate.SizeBytes is < 0 or > MaximumArtifactBytes ||
            candidate.Sha256 is { } sha256 &&
            (sha256.Length != 64 ||
             sha256.Any(value => !(char.IsAsciiDigit(value) || value is >= 'a' and <= 'f'))))
        {
            throw new PluginRuntimeException("Plugin MCP Artifact candidate identity is invalid.");
        }
    }

    private static McpArtifactCandidate ParseCandidate(JsonElement value)
    {
        if (value.ValueKind != JsonValueKind.Object)
        {
            throw new JsonException("Artifact candidate is not an object.");
        }

        var keys = value.EnumerateObject().Select(property => property.Name)
            .ToHashSet(StringComparer.Ordinal);
        var baseKeys = new HashSet<string>(
            ["producer_artifact_id", "relative_path", "display_name", "media_type"],
            StringComparer.Ordinal);
        var legacyBaseKeys = new HashSet<string>(
            ["producer_artifact_id", "relative_path", "display_name", "mime_type"],
            StringComparer.Ordinal);
        var integrityKeys = new HashSet<string>(["size_bytes", "sha256"], StringComparer.Ordinal);
        if (!keys.SetEquals(baseKeys) && !keys.SetEquals(legacyBaseKeys) &&
            !keys.SetEquals(baseKeys.Concat(integrityKeys)) &&
            !keys.SetEquals(legacyBaseKeys.Concat(integrityKeys)))
        {
            throw new JsonException("Artifact candidate fields are invalid.");
        }

        var mediaProperty = keys.Contains("media_type") ? "media_type" : "mime_type";
        var producerArtifactId = RequiredJsonString(value, "producer_artifact_id");
        var relativePath = RequiredJsonString(value, "relative_path");
        var displayName = RequiredJsonString(value, "display_name");
        var mediaType = RequiredJsonString(value, mediaProperty);
        long? sizeBytes = null;
        string? sha256 = null;
        if (keys.Contains("size_bytes"))
        {
            if (!value.GetProperty("size_bytes").TryGetInt64(out var declaredSize))
            {
                throw new JsonException("Artifact size is invalid.");
            }
            sizeBytes = declaredSize;
            sha256 = RequiredJsonString(value, "sha256");
        }

        return new McpArtifactCandidate(
            producerArtifactId, relativePath, displayName, mediaType, sizeBytes, sha256);
    }

    private static string RequiredJsonString(JsonElement value, string property) =>
        value.GetProperty(property).ValueKind == JsonValueKind.String
            ? value.GetProperty(property).GetString()
                ?? throw new JsonException($"Artifact {property} is empty.")
            : throw new JsonException($"Artifact {property} is not a string.");

    private static IReadOnlyList<string> MediaTypesForPath(string path) =>
        Path.GetExtension(path).ToLowerInvariant() switch
    {
        ".png" => ["image/png"],
        ".jpg" or ".jpeg" => ["image/jpeg"],
        ".pdf" => ["application/pdf"],
        ".json" =>
        [
            "application/json",
            "application/vnd.chatos.diagram+json",
            "application/vnd.chatos.solution-workspace+json",
        ],
        ".har" => ["application/json"],
        ".svg" => ["image/svg+xml"],
        ".puml" => ["text/vnd.plantuml"],
        ".md" => ["text/markdown"],
        ".txt" => ["text/plain"],
        ".csv" => ["text/csv"],
        ".zip" => ["application/zip"],
        ".docx" => ["application/vnd.openxmlformats-officedocument.wordprocessingml.document"],
        ".xlsx" => ["application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"],
        ".pptx" => ["application/vnd.openxmlformats-officedocument.presentationml.presentation"],
        _ => [],
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

    private sealed record McpArtifactCandidate(
        string ProducerArtifactId,
        string RelativePath,
        string DisplayName,
        string MediaType,
        long? SizeBytes,
        string? Sha256);
}
