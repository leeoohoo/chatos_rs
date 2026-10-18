using System.Security.Cryptography;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Tests;

public sealed class LocallyManagedPluginCatalogTests : IDisposable
{
    private readonly string _root = Path.Combine(
        Path.GetTempPath(),
        $"chatos-local-plugin-catalog-{Guid.NewGuid():N}");

    [Fact]
    public async Task LegacyCustomInstallationIsDiscoveredWithManifestMetadataAfterRestart()
    {
        var installation = CreateInstallation();
        var manifestPath = Path.Combine(installation, "chatos.plugin.json");
        var record = new InstalledPluginRecord(
            "custom-restart-fixture",
            $"custom:{new string('a', 64)}",
            "1.2.3",
            new string('a', 64),
            installation,
            DateTimeOffset.Parse("2026-10-17T12:00:00Z"),
            [],
            new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["chatos.plugin.json"] = HashFile(manifestPath),
            },
            "portable-restart-fixture");

        var catalog = await LocalPluginManagementService.MergeCatalogAsync(
            Array.Empty<ConnectorPluginSource>(),
            [record],
            new PluginManifestLoader(Path.Combine(_root, "runtime")));

        var plugin = Assert.Single(catalog);
        Assert.Equal("Portable Restart Fixture", plugin.DisplayName);
        Assert.Equal("Portable Skill restart fixture", plugin.Description);
        Assert.Equal("Local Plugin", plugin.Category);
        Assert.Equal("Local", plugin.Publisher);
        Assert.Equal("1.2.3", plugin.LatestVersion);
        Assert.True(plugin.Installed);
        Assert.True(plugin.Enabled);
        Assert.False(plugin.InstallAvailable);
        Assert.False(plugin.UpdateAvailable);
        Assert.True(record.IsLocallyManaged);
    }

    [Fact]
    public async Task LocalEnabledStateIsProjectedWithoutCallingTheMarketplace()
    {
        var installation = CreateInstallation();
        var manifestPath = Path.Combine(installation, "chatos.plugin.json");
        var record = new InstalledPluginRecord(
            "custom-disabled-fixture",
            $"custom:{new string('b', 64)}",
            "1.2.3",
            new string('b', 64),
            installation,
            DateTimeOffset.Parse("2026-10-17T12:00:00Z"),
            [],
            new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["chatos.plugin.json"] = HashFile(manifestPath),
            },
            "portable-restart-fixture",
            "localFolder",
            "Portable Agent Plugin · Local folder",
            false);

        var catalog = await LocalPluginManagementService.MergeCatalogAsync(
            Array.Empty<ConnectorPluginSource>(),
            [record],
            new PluginManifestLoader(Path.Combine(_root, "runtime")));

        Assert.False(Assert.Single(catalog).Enabled);
        Assert.False(record.IsLocallyEnabled);
    }

    [Fact]
    public async Task TamperedLocalManifestRemainsVisibleButFailsClosed()
    {
        var installation = CreateInstallation();
        var manifestPath = Path.Combine(installation, "chatos.plugin.json");
        var record = new InstalledPluginRecord(
            "custom-tampered-fixture",
            $"custom:{new string('c', 64)}",
            "1.2.3",
            new string('c', 64),
            installation,
            DateTimeOffset.Parse("2026-10-17T12:00:00Z"),
            [],
            new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["chatos.plugin.json"] = HashFile(manifestPath),
            },
            "portable-restart-fixture");
        await File.AppendAllTextAsync(manifestPath, " ");

        var catalog = await LocalPluginManagementService.MergeCatalogAsync(
            Array.Empty<ConnectorPluginSource>(),
            [record],
            new PluginManifestLoader(Path.Combine(_root, "runtime")));

        var plugin = Assert.Single(catalog);
        Assert.Equal("portable-restart-fixture", plugin.DisplayName);
        Assert.Contains("integrity validation failed", plugin.Description, StringComparison.Ordinal);
    }

    [Fact]
    public async Task LocalEnablementIsPersistedWithoutCallingTheGateway()
    {
        var database = new LocalStateDatabase(Path.Combine(_root, "enablement.db"));
        await database.InitializeAsync();
        var store = new SqliteInstalledPluginStore(database);
        var record = new InstalledPluginRecord(
            "custom-enable-fixture",
            $"custom:{new string('e', 64)}",
            "1.0.0",
            new string('e', 64),
            Path.Combine(_root, "installed"),
            DateTimeOffset.Parse("2026-10-17T12:00:00Z"),
            [],
            PluginKey: "custom-enable-fixture",
            SourceKind: "localFolder",
            LocalEnabled: false);
        await store.SaveAsync(record);
        var service = new LocalPluginManagementService(
            null!,
            null!,
            null!,
            store,
            new PluginManifestLoader(Path.Combine(_root, "runtime")),
            null!,
            null!,
            null!,
            null!);

        await service.SetEnabledAsync(record.PluginId, true);

        var restored = Assert.IsType<InstalledPluginRecord>(await store.GetAsync(record.PluginId));
        Assert.True(restored.IsLocallyEnabled);
    }

    public void Dispose()
    {
        try
        {
            if (Directory.Exists(_root)) Directory.Delete(_root, recursive: true);
        }
        catch (IOException)
        {
        }
    }

    private string CreateInstallation()
    {
        var installation = Path.Combine(_root, Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(installation);
        File.WriteAllText(Path.Combine(installation, "chatos.plugin.json"), """
            {
              "schemaVersion": 3,
              "name": "portable-restart-fixture",
              "version": "1.2.3",
              "description": "Portable Skill restart fixture",
              "interface": { "displayName": "Portable Restart Fixture" },
              "skills": ["skills/reviewer"]
            }
            """);
        return installation;
    }

    private static string HashFile(string path) =>
        Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path))).ToLowerInvariant();
}
