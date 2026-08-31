using ChatOS.Connector.Persistence;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Tests;

public sealed class SqliteInstalledPluginStoreTests
{
    [Fact]
    public async Task SavesReplacesListsAndDeletesInstallationRecords()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"chatos-plugin-store-{Guid.NewGuid():N}");
        try
        {
            var database = new LocalStateDatabase(Path.Combine(directory, "state.db"));
            await database.InitializeAsync();
            var store = new SqliteInstalledPluginStore(database);
            var first = new InstalledPluginRecord(
                "plugin-1",
                "release-1",
                "1.0.0",
                new string('a', 64),
                "C:\\Plugins\\one",
                DateTimeOffset.Parse("2026-08-30T10:00:00Z"),
                ["process.spawn"]);
            await store.SaveAsync(first);

            var storedFirst = await store.GetAsync("plugin-1");
            Assert.NotNull(storedFirst);
            Assert.Equal(first.PluginId, storedFirst.PluginId);
            Assert.Equal(first.ReleaseId, storedFirst.ReleaseId);
            Assert.Equal(first.DeclaredPermissions, storedFirst.DeclaredPermissions);

            var updated = first with
            {
                ReleaseId = "release-2",
                Version = "2.0.0",
                InstallationPath = "C:\\Plugins\\two",
                InstalledAt = first.InstalledAt.AddMinutes(1),
            };
            await store.SaveAsync(updated);

            var storedUpdated = await store.GetAsync("plugin-1");
            Assert.NotNull(storedUpdated);
            Assert.Equal(updated.ReleaseId, storedUpdated.ReleaseId);
            Assert.Equal(updated.Version, storedUpdated.Version);
            Assert.Equal(updated.InstallationPath, storedUpdated.InstallationPath);
            Assert.Single(await store.ListAsync());
            await store.DeleteAsync("plugin-1");
            Assert.Null(await store.GetAsync("plugin-1"));
        }
        finally
        {
            try
            {
                if (Directory.Exists(directory))
                {
                    Directory.Delete(directory, recursive: true);
                }
            }
            catch (IOException)
            {
            }
        }
    }
}
