using ChatOS.Connector.Persistence;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class SqliteConnectorModelSettingsStoreTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        "chatos-model-settings-tests",
        Guid.NewGuid().ToString("N"));

    public Task InitializeAsync()
    {
        Directory.CreateDirectory(_directory);
        return Task.CompletedTask;
    }

    public Task DisposeAsync()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, true);
        return Task.CompletedTask;
    }

    [Fact]
    public async Task ReturnsDefaultsAndRoundTripsSelection()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        var store = new SqliteConnectorModelSettingsStore(database);

        Assert.Equal(ConnectorModelSettings.Default, await store.LoadAsync());

        await store.SaveAsync(new ConnectorModelSettings(7, " model-one "));

        Assert.Equal(new ConnectorModelSettings(7, "model-one"), await store.LoadAsync());
    }

    [Fact]
    public async Task ClampsRetryCountBeforePersisting()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "clamp.db"));
        await database.InitializeAsync();
        var store = new SqliteConnectorModelSettingsStore(database);

        await store.SaveAsync(new ConnectorModelSettings(99, null));

        Assert.Equal(10, (await store.LoadAsync()).ModelRequestMaxRetries);
    }
}
