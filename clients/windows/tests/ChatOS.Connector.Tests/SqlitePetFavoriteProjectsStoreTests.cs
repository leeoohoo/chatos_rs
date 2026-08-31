using ChatOS.Connector.Persistence;

namespace ChatOS.Connector.Tests;

public sealed class SqlitePetFavoriteProjectsStoreTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        "chatos-pet-favorites-tests",
        Guid.NewGuid().ToString("N"));

    public Task InitializeAsync()
    {
        Directory.CreateDirectory(_directory);
        return Task.CompletedTask;
    }

    public Task DisposeAsync()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, recursive: true);
        return Task.CompletedTask;
    }

    [Fact]
    public async Task Saves_sorted_unique_project_ids()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        var store = new SqlitePetFavoriteProjectsStore(database);

        await store.SaveAsync(["project-two", " project-one ", "project-two"]);

        Assert.Equal(["project-one", "project-two"], await store.LoadAsync());
    }

    [Fact]
    public async Task Invalid_json_returns_an_empty_collection()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "invalid.db"));
        await database.InitializeAsync();
        await using (var connection = await database.OpenConnectionAsync())
        {
            var command = connection.CreateCommand();
            command.CommandText = """
                INSERT INTO pet_preferences(key, value, updated_at)
                VALUES ('favorite_project_ids_v1', '{broken', 'now');
                """;
            await command.ExecuteNonQueryAsync();
        }

        Assert.Empty(await new SqlitePetFavoriteProjectsStore(database).LoadAsync());
    }
}
