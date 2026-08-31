using ChatOS.Connector.Persistence;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class SqlitePetWindowPlacementStoreTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        "chatos-pet-placement-tests",
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
    public async Task Round_trips_physical_window_coordinates()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        var store = new SqlitePetWindowPlacementStore(database);
        var expected = new PetWindowPlacement(-1440, 860);

        await store.SaveAsync(expected);

        Assert.Equal(expected, await store.LoadAsync());
    }

    [Fact]
    public async Task Invalid_placement_json_is_ignored()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "invalid.db"));
        await database.InitializeAsync();
        await using (var connection = await database.OpenConnectionAsync())
        {
            var command = connection.CreateCommand();
            command.CommandText = """
                INSERT INTO pet_preferences(key, value, updated_at)
                VALUES ('window_placement_v1', '{broken', 'now');
                """;
            await command.ExecuteNonQueryAsync();
        }

        var store = new SqlitePetWindowPlacementStore(database);

        Assert.Null(await store.LoadAsync());
    }
}
