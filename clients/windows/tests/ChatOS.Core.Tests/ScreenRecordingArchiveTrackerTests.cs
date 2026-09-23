using ChatOS.Core.Domain;

namespace ChatOS.Core.Tests;

public sealed class ScreenRecordingArchiveTrackerTests
{
    [Fact]
    public void ReturnsOnlyAStableCompletedMp4CreatedForThisRecording()
    {
        var startedAt = new DateTimeOffset(2026, 9, 21, 10, 0, 0, TimeSpan.Zero);
        var tracker = new ScreenRecordingArchiveTracker(startedAt);
        var candidate = new ScreenRecordingFileCandidate(
            @"C:\Videos\Screen Recordings\capture.mp4", 2_048, startedAt.AddSeconds(1));

        Assert.Null(tracker.Observe([candidate], startedAt.AddSeconds(1)));
        Assert.Null(tracker.Observe([candidate with { Length = 4_096 }], startedAt.AddSeconds(2)));
        Assert.Equal(candidate.Path, tracker.Observe(
            [candidate with { Length = 4_096 }], startedAt.AddSeconds(4))?.Path);
    }

    [Fact]
    public void IgnoresOldEmptyAndNonMp4Files()
    {
        var startedAt = DateTimeOffset.UtcNow;
        var tracker = new ScreenRecordingArchiveTracker(startedAt);
        var candidates = new[]
        {
            new ScreenRecordingFileCandidate("old.mp4", 10, startedAt.AddMinutes(-1)),
            new ScreenRecordingFileCandidate("empty.mp4", 0, startedAt),
            new ScreenRecordingFileCandidate("capture.webm", 100, startedAt),
        };

        Assert.Null(tracker.Observe(candidates, startedAt.AddMinutes(1)));
        Assert.Null(tracker.Observe(candidates, startedAt.AddMinutes(2)));
    }
}
