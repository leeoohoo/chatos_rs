using ChatOS.Connector.Persistence;

namespace ChatOS.Connector.Tests;

public sealed class SqliteQuickSearchUsageStoreTests : IDisposable
{
    private readonly string _directory = Path.Combine(Path.GetTempPath(), $"chatos-search-{Guid.NewGuid():N}");

    [Fact]
    public async Task PersistsAndIncrementsUsageAcrossStoreInstances()
    {
        Directory.CreateDirectory(_directory);
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        var first = new SqliteQuickSearchUsageStore(database);
        await first.RecordAsync("application:notepad");
        await first.RecordAsync("application:notepad");

        var second = new SqliteQuickSearchUsageStore(database);
        var values = await second.LoadAsync();

        Assert.Equal(2, values["application:notepad"].UseCount);
        Assert.True(values["application:notepad"].LastUsedAt <= DateTimeOffset.UtcNow);
    }

    public void Dispose()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, recursive: true);
    }
}
