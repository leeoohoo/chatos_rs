using System.Security.Cryptography;
using System.Text.Json;

namespace ChatOS.Connector.Plugins;

internal static partial class PluginSkillSnapshotLoader
{
    public static JsonElement CreateLocalSnapshot(
        InstalledPluginRecord record,
        string componentKey)
    {
        var installationPath = Path.GetFullPath(record.InstallationPath);
        VerifyPackageFile(
            record, installationPath, "chatos.plugin.json", MaximumManifestBytes);
        var manifestPath = ResolveRegularFile(
            installationPath, "chatos.plugin.json", MaximumManifestBytes);
        using var manifest = JsonDocument.Parse(File.ReadAllBytes(manifestPath));
        if (!manifest.RootElement.TryGetProperty("schemaVersion", out var schemaVersion) ||
            schemaVersion.GetInt32() != 3 ||
            !StringPropertyEquals(manifest.RootElement, "version", record.Version))
        {
            throw new PluginRuntimeException(
                "Plugin manifest does not match the installed Release.");
        }

        var skillPath = FindSkillPath(manifest.RootElement, componentKey);
        var relativeSkillPath = $"{skillPath}/SKILL.md";
        VerifyPackageFile(
            record, installationPath, relativeSkillPath, MaximumInstructionsBytes);
        var collectionPath = ResolveDirectory(installationPath, skillPath);
        var skillBytes = ReadRegularFile(
            collectionPath, "SKILL.md", MaximumInstructionsBytes);
        var resources = ResourceDescriptors(collectionPath);
        foreach (var resource in resources)
        {
            VerifyPackageFile(
                record,
                installationPath,
                $"{skillPath}/{resource.GetProperty("relative_path").GetString()}",
                MaximumResourceBytes);
        }

        var metadata = JsonSerializer.SerializeToElement(new
        {
            name = componentKey,
            description = string.Empty,
            role = "leaf",
            activation_policy = "model_or_user",
            context_mode = "inline",
            required_skills = Array.Empty<string>(),
            related_skills = Array.Empty<string>(),
            extra = new { },
        }, JsonOptions);
        var resourcesElement = JsonSerializer.SerializeToElement(resources, JsonOptions);
        var instructionsSha256 = Sha256(skillBytes);
        var resourceManifestSha256 = CanonicalSha256(resourcesElement);
        var payload = JsonSerializer.SerializeToElement(new Dictionary<string, object?>
        {
            ["protocol_version"] = ProtocolVersion,
            ["skill_id"] = componentKey,
            ["relative_skill_path"] = relativeSkillPath,
            ["metadata"] = metadata,
            ["instructions_sha256"] = instructionsSha256,
            ["resource_manifest_sha256"] = resourceManifestSha256,
        }, JsonOptions);
        return JsonSerializer.SerializeToElement(new Dictionary<string, object?>
        {
            ["protocol_version"] = ProtocolVersion,
            ["skill_id"] = componentKey,
            ["relative_skill_path"] = relativeSkillPath,
            ["metadata"] = metadata,
            ["instructions_sha256"] = instructionsSha256,
            ["resource_manifest_sha256"] = resourceManifestSha256,
            ["resources"] = resources,
            ["snapshot_sha256"] = CanonicalSha256(payload),
        }, JsonOptions);
    }

    private static void VerifyPackageFile(
        InstalledPluginRecord record,
        string installationPath,
        string relativePath,
        int maximumBytes)
    {
        if (record.PackageFileSha256 is null) return;
        var normalized = NormalizeRelativePath(relativePath);
        if (!record.PackageFileSha256.TryGetValue(normalized, out var expected))
        {
            throw new PluginRuntimeException(
                "Plugin Skill file is not covered by the installation checksums.");
        }
        var path = ResolveRegularFile(installationPath, normalized, maximumBytes);
        var actual = SHA256.HashData(File.ReadAllBytes(path));
        byte[] expectedBytes;
        try
        {
            expectedBytes = Convert.FromHexString(expected);
        }
        catch (FormatException exception)
        {
            throw new PluginRuntimeException(
                "Plugin Skill installation checksum is invalid.", exception);
        }
        if (!CryptographicOperations.FixedTimeEquals(actual, expectedBytes))
        {
            throw new PluginRuntimeException(
                "Plugin Skill file checksum changed after installation.");
        }
    }
}
