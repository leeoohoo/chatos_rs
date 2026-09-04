using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Relay;

namespace ChatOS.Connector.Plugins;

internal static class PluginSkillSnapshotLoader
{
    public const int ProtocolVersion = 2;
    private const int MaximumManifestBytes = 4 * 1024 * 1024;
    private const int MaximumInstructionsBytes = 256 * 1024;
    private const int MaximumResourceBytes = 1024 * 1024;
    private const int MaximumTotalResourceBytes = 4 * 1024 * 1024;
    private const int MaximumResourceCount = 256;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public static JsonElement Prepare(
        InstalledPluginRecord record,
        string componentKey,
        JsonElement expectedSnapshot,
        string runId,
        string adapterSessionId,
        DateTimeOffset now)
    {
        var validated = Validate(record, componentKey, expectedSnapshot);
        var sessionSha256 = CanonicalSha256(JsonSerializer.SerializeToElement(new
        {
            protocol_version = ProtocolVersion,
            run_id = runId,
            adapter_session_id = adapterSessionId,
            plugin_id = record.PluginId,
            release_id = record.ReleaseId,
            component_key = componentKey,
            snapshot_sha256 = validated.SnapshotSha256,
        }, JsonOptions));
        return JsonSerializer.SerializeToElement(new
        {
            protocol_version = ProtocolVersion,
            run_id = runId,
            plugin_id = record.PluginId,
            release_id = record.ReleaseId,
            version = record.Version,
            artifact_sha256 = record.ArtifactSha256,
            component_key = componentKey,
            skills = new[] { validated.CatalogSnapshot },
            commands = Array.Empty<object>(),
            agents = Array.Empty<object>(),
            operations = new[] { "skill_activate", "skill_read_resource" },
            adapter_session_id = adapterSessionId,
            session_sha256 = sessionSha256,
            expires_at = now.AddDays(8).ToUnixTimeSeconds(),
        }, JsonOptions);
    }

    public static JsonElement Activate(
        InstalledPluginRecord record,
        string componentKey,
        JsonElement expectedSnapshot)
    {
        var validated = Validate(record, componentKey, expectedSnapshot);
        var instructions = Encoding.UTF8.GetString(validated.SkillBytes);
        return JsonSerializer.SerializeToElement(new
        {
            skill_id = componentKey,
            instructions,
            instructions_sha256 = validated.InstructionsSha256,
            resource_manifest_sha256 = validated.ResourceManifestSha256,
            snapshot_sha256 = validated.SnapshotSha256,
            resources = validated.Resources,
        }, JsonOptions);
    }

    public static JsonElement ReadResource(
        InstalledPluginRecord record,
        string componentKey,
        JsonElement expectedSnapshot,
        string relativePath,
        int offset,
        int maximumCharacters)
    {
        var validated = Validate(record, componentKey, expectedSnapshot);
        var normalized = NormalizeRelativePath(relativePath);
        if (normalized == "SKILL.md")
        {
            throw new PluginRuntimeException("Plugin Skill resource is not part of the immutable resource index.");
        }

        var descriptor = validated.Resources.FirstOrDefault(value =>
            value.TryGetProperty("relative_path", out var path) && path.GetString() == normalized);
        if (descriptor.ValueKind == JsonValueKind.Undefined)
        {
            throw new PluginRuntimeException("Plugin Skill resource is not part of the immutable snapshot.");
        }

        var kind = descriptor.GetProperty("kind").GetString();
        if (kind is not ("reference" or "schema" or "other"))
        {
            throw new PluginRuntimeException("Plugin Skill resource is not a readable text resource.");
        }

        var bytes = ReadRegularFile(validated.CollectionPath, normalized, MaximumResourceBytes);
        var actualSha256 = Sha256(bytes);
        if (!FixedTimeEquals(actualSha256, descriptor.GetProperty("sha256").GetString()))
        {
            throw new PluginRuntimeException("Plugin Skill resource hash does not match the immutable snapshot.");
        }

        string text;
        try
        {
            text = new UTF8Encoding(false, true).GetString(bytes);
        }
        catch (DecoderFallbackException exception)
        {
            throw new PluginRuntimeException("Plugin Skill text resource is not UTF-8.", exception);
        }

        var characters = text.EnumerateRunes().Select(rune => rune.ToString()).ToArray();
        if (offset < 0 || offset > characters.Length)
        {
            throw new PluginRuntimeException("Plugin Skill resource offset is invalid.");
        }

        var limit = Math.Clamp(maximumCharacters, 1, 64_000);
        var end = Math.Min(offset + limit, characters.Length);
        var content = string.Concat(characters[offset..end]);
        return JsonSerializer.SerializeToElement(new
        {
            skill_id = componentKey,
            relative_path = normalized,
            sha256 = actualSha256,
            content,
            offset,
            next_offset = end < characters.Length ? end : (int?)null,
            truncated = end < characters.Length,
        }, JsonOptions);
    }

    private static ValidatedSnapshot Validate(
        InstalledPluginRecord record,
        string componentKey,
        JsonElement expectedSnapshot)
    {
        if (expectedSnapshot.ValueKind != JsonValueKind.Object ||
            !expectedSnapshot.TryGetProperty("protocol_version", out var protocol) ||
            protocol.GetInt32() != ProtocolVersion ||
            !StringPropertyEquals(expectedSnapshot, "skill_id", componentKey) ||
            !expectedSnapshot.TryGetProperty("metadata", out var metadata) ||
            metadata.ValueKind != JsonValueKind.Object ||
            !expectedSnapshot.TryGetProperty("resources", out var expectedResources) ||
            expectedResources.ValueKind != JsonValueKind.Array)
        {
            throw new PluginRuntimeException("Plugin Skill v2 immutable snapshot is invalid.");
        }

        var installationPath = Path.GetFullPath(record.InstallationPath);
        var manifestPath = ResolveRegularFile(installationPath, "chatos.plugin.json", MaximumManifestBytes);
        using var manifest = JsonDocument.Parse(File.ReadAllBytes(manifestPath));
        var root = manifest.RootElement;
        if (!root.TryGetProperty("schemaVersion", out var schemaVersion) || schemaVersion.GetInt32() != 3 ||
            !StringPropertyEquals(root, "version", record.Version))
        {
            throw new PluginRuntimeException("Plugin manifest does not match the installed Release.");
        }

        var skillPath = FindSkillPath(root, componentKey);
        var relativeSkillPath = $"{skillPath}/SKILL.md";
        if (!StringPropertyEquals(expectedSnapshot, "relative_skill_path", relativeSkillPath))
        {
            throw new PluginRuntimeException("Plugin Skill path does not match the immutable snapshot.");
        }

        var collectionPath = ResolveDirectory(installationPath, skillPath);
        var skillBytes = ReadRegularFile(collectionPath, "SKILL.md", MaximumInstructionsBytes);
        try
        {
            _ = new UTF8Encoding(false, true).GetString(skillBytes);
        }
        catch (DecoderFallbackException exception)
        {
            throw new PluginRuntimeException("Plugin Skill instructions are not UTF-8.", exception);
        }
        var instructionsSha256 = Sha256(skillBytes);
        if (!StringPropertyEquals(expectedSnapshot, "instructions_sha256", instructionsSha256))
        {
            throw new PluginRuntimeException("Plugin Skill instructions hash does not match the immutable snapshot.");
        }

        var resources = ResourceDescriptors(collectionPath);
        var resourcesElement = JsonSerializer.SerializeToElement(resources, JsonOptions);
        if (CanonicalJson.Serialize(resourcesElement) != CanonicalJson.Serialize(expectedResources))
        {
            throw new PluginRuntimeException("Plugin Skill resource index does not match the immutable snapshot.");
        }

        var resourceManifestSha256 = CanonicalSha256(resourcesElement);
        if (!StringPropertyEquals(expectedSnapshot, "resource_manifest_sha256", resourceManifestSha256))
        {
            throw new PluginRuntimeException("Plugin Skill resource digest does not match the immutable snapshot.");
        }

        var snapshotPayload = JsonSerializer.SerializeToElement(new Dictionary<string, object?>
        {
            ["protocol_version"] = ProtocolVersion,
            ["skill_id"] = componentKey,
            ["relative_skill_path"] = relativeSkillPath,
            ["metadata"] = metadata.Clone(),
            ["instructions_sha256"] = instructionsSha256,
            ["resource_manifest_sha256"] = resourceManifestSha256,
        }, JsonOptions);
        var snapshotSha256 = CanonicalSha256(snapshotPayload);
        if (!StringPropertyEquals(expectedSnapshot, "snapshot_sha256", snapshotSha256))
        {
            throw new PluginRuntimeException("Plugin Skill content digest does not match the immutable snapshot.");
        }

        var catalog = JsonSerializer.SerializeToElement(new Dictionary<string, object?>
        {
            ["protocol_version"] = ProtocolVersion,
            ["skill_id"] = componentKey,
            ["relative_skill_path"] = relativeSkillPath,
            ["metadata"] = metadata.Clone(),
            ["instructions_sha256"] = instructionsSha256,
            ["resource_manifest_sha256"] = resourceManifestSha256,
            ["resources"] = resources,
            ["snapshot_sha256"] = snapshotSha256,
        }, JsonOptions);
        return new ValidatedSnapshot(
            collectionPath,
            skillBytes,
            instructionsSha256,
            resourceManifestSha256,
            snapshotSha256,
            resources,
            catalog);
    }

    private static IReadOnlyList<JsonElement> ResourceDescriptors(string collectionPath)
    {
        var resources = new List<JsonElement>();
        long totalBytes = 0;
        foreach (var path in Directory.EnumerateFiles(collectionPath, "*", SearchOption.AllDirectories)
                     .Order(StringComparer.Ordinal))
        {
            var relative = Path.GetRelativePath(collectionPath, path).Replace('\\', '/');
            if (Path.GetFileName(path) == "SKILL.md" || IsHidden(relative))
            {
                continue;
            }
            if (resources.Count >= MaximumResourceCount || IsReparsePoint(path))
            {
                throw new PluginRuntimeException("Plugin Skill resource index is too large or unsafe.");
            }

            var bytes = ReadRegularFile(collectionPath, relative, MaximumResourceBytes);
            totalBytes += bytes.LongLength;
            if (totalBytes > MaximumTotalResourceBytes)
            {
                throw new PluginRuntimeException("Plugin Skill resources exceed the total size limit.");
            }
            resources.Add(JsonSerializer.SerializeToElement(new
            {
                relative_path = relative,
                kind = ResourceKind(relative),
                size_bytes = bytes.LongLength,
                sha256 = Sha256(bytes),
            }, JsonOptions));
        }
        return resources.OrderBy(value => value.GetProperty("relative_path").GetString(), StringComparer.Ordinal)
            .Select(value => value.Clone()).ToArray();
    }

    private static string FindSkillPath(JsonElement manifest, string componentKey)
    {
        if (!manifest.TryGetProperty("skills", out var skills) || skills.ValueKind != JsonValueKind.Array)
        {
            throw new PluginRuntimeException("Plugin manifest does not declare any Skills.");
        }

        var index = 0;
        foreach (var value in skills.EnumerateArray())
        {
            var path = value.ValueKind == JsonValueKind.String
                ? value.GetString()
                : value.ValueKind == JsonValueKind.Object && value.TryGetProperty("path", out var pathValue)
                    ? pathValue.GetString()
                    : null;
            if (!string.IsNullOrWhiteSpace(path))
            {
                var normalized = NormalizeRelativePath(path);
                if (ComponentKeyFromPath(normalized, "skills", index) == componentKey)
                {
                    return normalized;
                }
            }
            index++;
        }
        throw new PluginRuntimeException("The requested Plugin Skill component was not found.");
    }

    private static string ResolveDirectory(string root, string relativePath)
    {
        var normalized = NormalizeRelativePath(relativePath);
        var path = Path.GetFullPath(Path.Combine(root, normalized.Replace('/', Path.DirectorySeparatorChar)));
        EnsureBeneath(root, path);
        if (!Directory.Exists(path) || IsReparsePoint(path))
        {
            throw new PluginRuntimeException("Plugin Skill directory is missing or unsafe.");
        }
        return path;
    }

    private static string ResolveRegularFile(string root, string relativePath, int maximumBytes)
    {
        var normalized = NormalizeRelativePath(relativePath);
        var path = Path.GetFullPath(Path.Combine(root, normalized.Replace('/', Path.DirectorySeparatorChar)));
        EnsureBeneath(root, path);
        var info = new FileInfo(path);
        if (!info.Exists || info.Length < 0 || info.Length > maximumBytes || IsReparsePoint(path))
        {
            throw new PluginRuntimeException("Plugin Skill file is missing, unsafe, or too large.");
        }
        return path;
    }

    private static byte[] ReadRegularFile(string root, string relativePath, int maximumBytes)
    {
        var path = ResolveRegularFile(root, relativePath, maximumBytes);
        var bytes = File.ReadAllBytes(path);
        if (bytes.Length > maximumBytes)
        {
            throw new PluginRuntimeException("Plugin Skill file exceeds its size limit.");
        }
        return bytes;
    }

    private static void EnsureBeneath(string root, string path)
    {
        var rootPrefix = Path.TrimEndingDirectorySeparator(Path.GetFullPath(root)) + Path.DirectorySeparatorChar;
        if (!path.StartsWith(rootPrefix, StringComparison.OrdinalIgnoreCase))
        {
            throw new PluginRuntimeException("Plugin Skill path escapes the installation directory.");
        }
        for (var current = new DirectoryInfo(Path.GetDirectoryName(path)!);
             current is not null && current.FullName.StartsWith(rootPrefix, StringComparison.OrdinalIgnoreCase);
             current = current.Parent)
        {
            if (current.Exists && IsReparsePoint(current.FullName))
            {
                throw new PluginRuntimeException("Plugin Skill path crosses a reparse point.");
            }
        }
    }

    private static string NormalizeRelativePath(string? value)
    {
        var normalized = (value ?? string.Empty).Replace('\\', '/').Trim();
        while (normalized.StartsWith("./", StringComparison.Ordinal))
        {
            normalized = normalized[2..];
        }
        var parts = normalized.Split('/', StringSplitOptions.None);
        if (normalized.Length == 0 || Path.IsPathFullyQualified(normalized) ||
            parts.Any(part => part.Length == 0 || part is "." or ".."))
        {
            throw new PluginRuntimeException("Plugin Skill path is invalid.");
        }
        return normalized;
    }

    private static string ComponentKeyFromPath(string path, string fallback, int index)
    {
        var name = path.Trim('/').Split('/').LastOrDefault()?.Split('.')[0] ?? fallback;
        var normalized = new string(name.ToLowerInvariant().Select(character =>
            char.IsAsciiLetterOrDigit(character) ? character : '-').ToArray());
        while (normalized.Contains("--", StringComparison.Ordinal))
        {
            normalized = normalized.Replace("--", "-", StringComparison.Ordinal);
        }
        normalized = normalized.Trim('-');
        if (normalized.Length == 0)
        {
            normalized = fallback;
        }
        return index > 0 && normalized == fallback ? $"{normalized}-{index + 1}" : normalized;
    }

    private static string ResourceKind(string relativePath)
    {
        return relativePath.Split('/')[0] switch
        {
            "references" => "reference",
            "scripts" => "script",
            "assets" => "asset",
            _ when relativePath.EndsWith(".json", StringComparison.Ordinal) ||
                relativePath.EndsWith(".schema.json", StringComparison.Ordinal) => "schema",
            _ => "other",
        };
    }

    private static bool StringPropertyEquals(JsonElement value, string property, string expected) =>
        value.TryGetProperty(property, out var actual) && actual.ValueKind == JsonValueKind.String &&
        string.Equals(actual.GetString(), expected, StringComparison.Ordinal);

    private static bool IsHidden(string relativePath) =>
        relativePath.Split('/')
            .Any(segment => segment.Length > 0 && segment[0] == '.');

    private static bool IsReparsePoint(string path) =>
        (File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0;

    private static string CanonicalSha256(JsonElement value) =>
        Sha256(Encoding.UTF8.GetBytes(CanonicalJson.Serialize(value)));

    private static string Sha256(byte[] value) =>
        Convert.ToHexString(SHA256.HashData(value)).ToLowerInvariant();

    private static bool FixedTimeEquals(string actual, string? expected)
    {
        if (expected is null || actual.Length != expected.Length)
        {
            return false;
        }
        return CryptographicOperations.FixedTimeEquals(
            Encoding.ASCII.GetBytes(actual), Encoding.ASCII.GetBytes(expected));
    }

    private sealed record ValidatedSnapshot(
        string CollectionPath,
        byte[] SkillBytes,
        string InstructionsSha256,
        string ResourceManifestSha256,
        string SnapshotSha256,
        IReadOnlyList<JsonElement> Resources,
        JsonElement CatalogSnapshot);
}
