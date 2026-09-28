using ChatOS.Desktop.Features.MediaStudio;

namespace ChatOS.Api.Tests;

public sealed class StoryProjectStoreTests
{
    [Fact]
    public async Task SaveAndLoadKeepProjectsInsideOwningAccount()
    {
        using var folder = new TemporaryFolder();
        var store = new StoryProjectStore(folder.Path);
        var project = Project("A story");

        await store.SaveAsync("owner-a", project);

        var restored = Assert.Single(await store.LoadAsync("owner-a"));
        Assert.Equal(project.Id, restored.Id);
        Assert.Equal(project.Title, restored.Title);
        Assert.Equal(project.Source, restored.Source);
        Assert.Equal(project.Segments, restored.Segments);
        Assert.Empty(await store.LoadAsync("owner-b"));
    }

    [Fact]
    public async Task LoadSkipsUnreadableProjectAndPreservesValidProjects()
    {
        using var folder = new TemporaryFolder();
        var store = new StoryProjectStore(folder.Path);
        var project = Project("Valid story");
        await store.SaveAsync("owner-a", project);
        var ownerFolder = Directory.GetDirectories(folder.Path).Single();
        var corruptFolder = Path.Combine(ownerFolder, Guid.NewGuid().ToString());
        Directory.CreateDirectory(corruptFolder);
        await File.WriteAllTextAsync(Path.Combine(corruptFolder, "project.json"), "not json");

        var projects = await store.LoadAsync("owner-a");

        Assert.Equal(project.Id, Assert.Single(projects).Id);
        Assert.True(File.Exists(Path.Combine(corruptFolder, "project.json")));
    }

    [Fact]
    public async Task ImportedAssetsStayInsideAccountProjectDirectory()
    {
        using var folder = new TemporaryFolder();
        var store = new StoryProjectStore(folder.Path);
        var project = Project("Asset story");
        await store.SaveAsync("owner-a", project);
        var source = Path.Combine(folder.Path, "source.png");
        await File.WriteAllBytesAsync(source, [1, 2, 3]);

        var relative = await store.ImportAssetAsync(
            "owner-a", project.Id, "../unsafe/segment", source, false);
        var resolved = store.ResolveAssetPath("owner-a", project.Id, relative);

        Assert.NotNull(resolved);
        Assert.True(File.Exists(resolved));
        Assert.Equal(new byte[] { 1, 2, 3 }, await File.ReadAllBytesAsync(resolved!));
        Assert.Null(store.ResolveAssetPath("owner-a", project.Id, "../../outside.png"));
    }

    [Fact]
    public void ProjectValidationRejectsUnsafeAssetReferences()
    {
        var project = Project("Unsafe") with
        {
            Segments =
            [
                new StorySegmentDocument(
                    "segment-1", "Shot", "Narrative", "Image", "Video", 4,
                    "../../secret.png", null, null),
            ],
        };

        Assert.Throws<InvalidDataException>(project.Validate);
    }

    private static StoryProjectDocument Project(string title)
    {
        var now = new DateTimeOffset(2026, 9, 28, 8, 0, 0, TimeSpan.Zero);
        return new StoryProjectDocument(
            Guid.NewGuid(), StoryProjectDocument.CurrentVersion, title, "Description", "Source", "Style",
            "16:9", "text-model", "image-model", "video-model",
            [new StorySegmentDocument("segment-1", "Shot", "Narrative", "Image", "Video", 4, null, null, null)],
            now, now);
    }

    private sealed class TemporaryFolder : IDisposable
    {
        public TemporaryFolder()
        {
            Path = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                $"chatos-story-store-{Guid.NewGuid():N}");
            Directory.CreateDirectory(Path);
        }

        public string Path { get; }

        public void Dispose()
        {
            if (Directory.Exists(Path)) Directory.Delete(Path, true);
        }
    }
}
