using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class SqliteAppPreferencesStoreTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        "chatos-app-preferences-tests",
        Guid.NewGuid().ToString("N"));

    public Task InitializeAsync()
    {
        Directory.CreateDirectory(_directory);
        return Task.CompletedTask;
    }

    public Task DisposeAsync()
    {
        if (Directory.Exists(_directory))
        {
            Directory.Delete(_directory, recursive: true);
        }

        return Task.CompletedTask;
    }

    [Fact]
    public async Task RoundTripsPreferencesThroughUiState()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        var store = new SqliteAppPreferencesStore(database);
        var expected = new AppPreferences(
            InterfaceLanguage.English,
            InterfaceTheme.Dark,
            1.15,
            false);

        await store.SaveAsync(expected);
        var loaded = await store.LoadAsync();

        Assert.Equal(expected, loaded);
    }

    [Fact]
    public async Task InvalidJsonFallsBackToNoPreferences()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "invalid.db"));
        await database.InitializeAsync();
        await using (var connection = await database.OpenConnectionAsync())
        {
            var command = connection.CreateCommand();
            command.CommandText = """
                INSERT INTO ui_state(key, value, updated_at)
                VALUES ('app_preferences_v1', '{broken', 'now');
                """;
            await command.ExecuteNonQueryAsync();
        }

        var store = new SqliteAppPreferencesStore(database);

        Assert.Null(await store.LoadAsync());
    }
}
