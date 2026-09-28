using ChatOS.Core.Domain;
using ChatOS.Desktop.Features.MediaStudio;

namespace ChatOS.Api.Tests;

public sealed class MediaStudioHistoryStoreTests
{
    [Fact]
    public async Task SaveAndLoadKeepHistoryInsideOwningAccount()
    {
        using var folder = new TemporaryFolder();
        var store = new MediaStudioHistoryStore(
            new NoNetworkHttpClientFactory(),
            folder.Path);
        var createdAt = new DateTimeOffset(2026, 9, 28, 8, 30, 0, TimeSpan.Zero);
        var result = new ImageGenerationResult(
            "result-1",
            "model-config",
            "image-v1",
            createdAt,
            [new GeneratedMediaAsset("image-1", "image/png", "AQID", null, "better fox")]);

        var saved = await store.SaveAsync("owner-a", "draw a fox", result);
        var ownerHistory = await store.LoadAsync("owner-a");
        var otherHistory = await store.LoadAsync("owner-b");

        Assert.Equal("draw a fox", saved.Prompt);
        Assert.True(File.Exists(Assert.Single(saved.Images).FilePath));
        var restored = Assert.Single(ownerHistory);
        Assert.Equal(createdAt, restored.CreatedAt);
        Assert.Equal("better fox", Assert.Single(restored.Images).RevisedPrompt);
        Assert.Empty(otherHistory);
    }

    [Fact]
    public async Task LoadSkipsCorruptManifestWithoutLosingValidEntries()
    {
        using var folder = new TemporaryFolder();
        var store = new MediaStudioHistoryStore(
            new NoNetworkHttpClientFactory(),
            folder.Path);
        var result = new ImageGenerationResult(
            "result-1",
            "model-config",
            "image-v1",
            DateTimeOffset.UtcNow,
            [new GeneratedMediaAsset("image-1", "image/png", "AQID", null, null)]);
        var saved = await store.SaveAsync("owner-a", "valid", result);
        var entryFolder = System.IO.Path.GetDirectoryName(Assert.Single(saved.Images).FilePath)!;
        var ownerFolder = System.IO.Path.GetDirectoryName(entryFolder)!;
        var corruptFolder = System.IO.Path.Combine(ownerFolder, "corrupt");
        Directory.CreateDirectory(corruptFolder);
        await File.WriteAllTextAsync(System.IO.Path.Combine(corruptFolder, "entry.json"), "not json");

        var history = await store.LoadAsync("owner-a");

        Assert.Single(history);
        Assert.Equal("valid", history[0].Prompt);
    }

    private sealed class NoNetworkHttpClientFactory : IHttpClientFactory
    {
        public HttpClient CreateClient(string name) =>
            throw new InvalidOperationException("The test should not access the network.");
    }

    private sealed class TemporaryFolder : IDisposable
    {
        public TemporaryFolder()
        {
            Path = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                $"chatos-media-history-{Guid.NewGuid():N}");
            Directory.CreateDirectory(Path);
        }

        public string Path { get; }

        public void Dispose()
        {
            if (Directory.Exists(Path)) Directory.Delete(Path, true);
        }
    }
}
