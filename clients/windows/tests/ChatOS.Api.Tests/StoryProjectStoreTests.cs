using ChatOS.Core.Domain;
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
        Assert.Equal("Keep the palette warm", restored.CreativeRequirements);
        var restoredSegment = Assert.Single(restored.Segments);
        Assert.Equal("segment-1", restoredSegment.Id);
        Assert.Equal(StorySegmentKind.Transition, restoredSegment.Kind);
        Assert.Equal(new[] { "hero" }, restoredSegment.ResourceIds);
        Assert.Equal("Enter from the hall", restoredSegment.ContinuityIn);
        Assert.Equal("Stop beside the window", restoredSegment.ContinuityOut);
        Assert.Equal("0-4s: dolly in", restoredSegment.ShotPlan);
        Assert.Equal("hero", Assert.Single(restored.Resources).Id);
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

    [Fact]
    public void ProjectValidationRejectsUnknownSegmentResource()
    {
        var project = Project("Unknown resource") with
        {
            Segments =
            [
                new StorySegmentDocument(
                    "segment-1", "Shot", "Narrative", "Image", "Video", 4,
                    null, null, null)
                {
                    ResourceIds = ["missing"],
                },
            ],
        };

        Assert.Throws<InvalidDataException>(project.Validate);
    }

    [Fact]
    public async Task PlanningRunsPersistRunningAndDraftReadyStates()
    {
        using var folder = new TemporaryFolder();
        var store = new StoryProjectStore(folder.Path);
        var projectId = Guid.NewGuid();
        var running = PlanningRun(projectId, StoryPlanningRunStatus.Running, null);
        var draftReady = PlanningRun(projectId, StoryPlanningRunStatus.DraftReady, PlanningDraft()) with
        {
            UpdatedAt = running.UpdatedAt.AddMinutes(1),
        };

        await store.SavePlanningRunAsync("owner-a", running);
        await store.SavePlanningRunAsync("owner-a", draftReady);

        var restored = await store.LoadPlanningRunsAsync("owner-a", projectId);
        Assert.Equal(2, restored.Count);
        Assert.Equal(draftReady.Id, restored[0].Id);
        Assert.Equal(StoryPlanningRunStatus.DraftReady, restored[0].Status);
        Assert.Equal(StorySegmentKind.Transition.ToString().ToLowerInvariant(),
            Assert.Single(restored[0].Draft!.Segments).Kind);
        Assert.True(restored[1].CanResume);
        Assert.Empty(await store.LoadPlanningRunsAsync("owner-b", projectId));
    }

    [Fact]
    public async Task PendingVideoGuidancePersistsForSafeResume()
    {
        using var folder = new TemporaryFolder();
        var store = new StoryProjectStore(folder.Path);
        var project = Project("Video edit") with
        {
            Segments =
            [
                Project("unused").Segments[0] with
                {
                    VideoAsset = "assets/segment-1/source.mp4",
                    PendingVideoJobId = "job-edit",
                    PendingVideoJobStatus = "processing",
                    PendingVideoRequestDigest = new string('a', 64),
                    PendingVideoGuidance = "source-video",
                },
            ],
        };

        await store.SaveAsync("owner-a", project);

        var restored = Assert.Single(await store.LoadAsync("owner-a"));
        var segment = Assert.Single(restored.Segments);
        Assert.Equal("source-video", segment.PendingVideoGuidance);
        Assert.Equal("job-edit", segment.PendingVideoJobId);
    }

    [Fact]
    public async Task PlanningRunLoadSkipsCorruptRecordWithoutDeletingIt()
    {
        using var folder = new TemporaryFolder();
        var store = new StoryProjectStore(folder.Path);
        var projectId = Guid.NewGuid();
        var run = PlanningRun(projectId, StoryPlanningRunStatus.DraftReady, PlanningDraft());
        await store.SavePlanningRunAsync("owner-a", run);
        var planningFolder = Directory.GetDirectories(folder.Path, "planning-runs", SearchOption.AllDirectories).Single();
        var corruptPath = Path.Combine(planningFolder, "corrupt.json");
        await File.WriteAllTextAsync(corruptPath, "not json");

        var restored = await store.LoadPlanningRunsAsync("owner-a", projectId);

        Assert.Equal(run.Id, Assert.Single(restored).Id);
        Assert.True(File.Exists(corruptPath));
    }

    private static StoryProjectDocument Project(string title)
    {
        var now = new DateTimeOffset(2026, 9, 28, 8, 0, 0, TimeSpan.Zero);
        return new StoryProjectDocument(
            Guid.NewGuid(), StoryProjectDocument.CurrentVersion, title, "Description", "Source", "Summary", "Style",
            "16:9", "text-model", "image-model", "video-model",
            [new StorySegmentDocument("segment-1", "Shot", "Narrative", "Image", "Video", 4, null, null, null)
                {
                    ResourceIds = ["hero"],
                    Kind = StorySegmentKind.Transition,
                    ContinuityIn = "Enter from the hall",
                    ContinuityOut = "Stop beside the window",
                    ShotPlan = "0-4s: dolly in",
                }],
            now, now)
        {
            CreativeRequirements = "Keep the palette warm",
            Resources = [new StoryResourceDocument("hero", StoryResourceKind.Character, "Hero", "Lead", "Hero portrait", null)],
        };
    }

    private static StoryPlanningRunDocument PlanningRun(
        Guid projectId,
        StoryPlanningRunStatus status,
        StoryPlanningResult? draft)
    {
        var now = new DateTimeOffset(2026, 9, 28, 9, 0, 0, TimeSpan.Zero);
        return new StoryPlanningRunDocument(
            Guid.NewGuid(), projectId, new string('a', 64),
            new StoryPlanningRequest("text-model", "Story", "Description", "Source", "Style", "16:9"),
            status, draft, null, now, now);
    }

    private static StoryPlanningResult PlanningDraft() => new(
        "Summary",
        [new PlannedStoryResource("station", "scene", "Station", "Old station", "Station exterior")],
        [new PlannedStorySegment("transition", "Travel", "Time passes", "Train window", "Dissolve", 3, ["station"])]);

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
