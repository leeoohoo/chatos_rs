using System.Security.Cryptography;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Tests;

public sealed class PluginSkillSnapshotLoaderTests
{
    [Fact]
    public void CreatesAndValidatesLocalSnapshotFromInstalledPackageHashes()
    {
        var root = Path.Combine(Path.GetTempPath(), $"chatos-skill-{Guid.NewGuid():N}");
        try
        {
            var skill = Path.Combine(root, "skills", "fixture-skill");
            var references = Path.Combine(skill, "references");
            Directory.CreateDirectory(references);
            File.WriteAllText(Path.Combine(root, "chatos.plugin.json"),
                """{"schemaVersion":3,"name":"fixture","version":"1.0.0","skills":["skills/fixture-skill"],"mcpServers":{}}""");
            File.WriteAllText(Path.Combine(skill, "SKILL.md"), "# Fixture\nUse the guide.\n");
            File.WriteAllText(Path.Combine(references, "guide.md"), "# Guide\nVerified.\n");
            var hashes = new Dictionary<string, string>(StringComparer.Ordinal)
            {
                ["chatos.plugin.json"] = Hash(Path.Combine(root, "chatos.plugin.json")),
                ["skills/fixture-skill/SKILL.md"] = Hash(Path.Combine(skill, "SKILL.md")),
                ["skills/fixture-skill/references/guide.md"] =
                    Hash(Path.Combine(references, "guide.md")),
            };
            var record = new InstalledPluginRecord(
                "plugin-1", "release-1", "1.0.0", new string('a', 64), root,
                DateTimeOffset.UtcNow, [], hashes);

            var snapshot = PluginSkillSnapshotLoader.CreateLocalSnapshot(
                record, "fixture-skill");
            var activated = PluginSkillSnapshotLoader.Activate(
                record, "fixture-skill", snapshot);
            var resource = PluginSkillSnapshotLoader.ReadResource(
                record, "fixture-skill", snapshot, "references/guide.md", 0, 64);

            Assert.Contains("Use the guide", activated.GetProperty("instructions").GetString()!);
            Assert.Contains("Verified", resource.GetProperty("content").GetString()!);
            File.WriteAllText(Path.Combine(references, "guide.md"), "tampered");
            Assert.Throws<PluginRuntimeException>(() =>
                PluginSkillSnapshotLoader.CreateLocalSnapshot(record, "fixture-skill"));
        }
        finally
        {
            if (Directory.Exists(root)) Directory.Delete(root, recursive: true);
        }
    }

    private static string Hash(string path) =>
        Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path))).ToLowerInvariant();
}
