using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class SqliteClipboardHistoryStoreTests : IDisposable
{
    private readonly string _directory = Path.Combine(Path.GetTempPath(), $"chatos-clipboard-{Guid.NewGuid():N}");

    [Fact]
    public async Task StoresDeduplicatesPinsAndRestoresAllPayloadKinds()
    {
        var database = await CreateDatabaseAsync();
        var store = new SqliteClipboardHistoryStore(database);
        var first = await store.StoreAsync(new(ClipboardHistoryKind.Text, Text: "hello"), "Editor");
        var duplicate = await store.StoreAsync(new(ClipboardHistoryKind.Text, Text: "hello"), "Terminal");
        var files = await store.StoreAsync(new(
            ClipboardHistoryKind.Files,
            FilePaths: [Path.Combine(_directory, "one.txt"), Path.Combine(_directory, "two.txt")]), null);
        var imageBytes = new byte[] { 1, 2, 3, 4 };
        var image = await store.StoreAsync(new(ClipboardHistoryKind.Image, ImageBytes: imageBytes), null);

        Assert.Equal(first.Id, duplicate.Id);
        Assert.Equal("Terminal", duplicate.SourceApplication);
        Assert.Equal("hello", (await store.ReadPayloadAsync(first.Id))?.Text);
        Assert.Equal(2, (await store.ReadPayloadAsync(files.Id))?.FilePaths?.Count);
        Assert.Equal(imageBytes, (await store.ReadPayloadAsync(image.Id))?.ImageBytes);

        await store.SetPinnedAsync(first.Id, true);
        var entries = await store.ListAsync();
        Assert.True(entries[0].IsPinned);
        Assert.Equal(first.Id, entries[0].Id);
    }

    [Fact]
    public async Task PruneKeepsPinnedEntriesAndHonorsCountLimit()
    {
        var database = await CreateDatabaseAsync();
        var store = new SqliteClipboardHistoryStore(database);
        var pinned = await store.StoreAsync(new(ClipboardHistoryKind.Text, Text: "keep"), null);
        await store.SetPinnedAsync(pinned.Id, true);
        for (var index = 0; index < 4; index++)
            await store.StoreAsync(new(ClipboardHistoryKind.Text, Text: $"item-{index}"), null);

        await store.PruneAsync(DateTimeOffset.UtcNow.AddDays(-30), unpinnedLimit: 2);

        var entries = await store.ListAsync();
        Assert.Equal(3, entries.Count);
        Assert.Contains(entries, value => value.Id == pinned.Id && value.IsPinned);
    }

    private async Task<LocalStateDatabase> CreateDatabaseAsync()
    {
        Directory.CreateDirectory(_directory);
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        return database;
    }

    public void Dispose()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, recursive: true);
    }
}
