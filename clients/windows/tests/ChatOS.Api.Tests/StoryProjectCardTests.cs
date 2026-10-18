using ChatOS.Desktop.Features.MediaStudio;

namespace ChatOS.Api.Tests;

public sealed class StoryProjectCardTests
{
    [Fact]
    public void StatusDistinguishesDraftProductionReviewAndCompleteProjects()
    {
        AssertStatus([], "待规划", StoryProjectStatusTone.Draft);
        AssertStatus([Segment()], "制作中", StoryProjectStatusTone.Production);
        AssertStatus([Segment() with { PendingVideoJobId = "job-1" }],
            "任务待核对", StoryProjectStatusTone.Review);
        AssertStatus([Segment() with { VideoAsset = "video.mp4" }],
            "已完成", StoryProjectStatusTone.Complete);
    }

    private static void AssertStatus(
        IReadOnlyList<StorySegmentDocument> segments,
        string label,
        StoryProjectStatusTone tone)
    {
        var now = DateTimeOffset.UtcNow;
        var project = new StoryProjectDocument(
            Guid.NewGuid(), StoryProjectDocument.CurrentVersion, "Story", string.Empty,
            string.Empty, string.Empty, "Style", "16:9", "text", "image", "video",
            segments, now, now);
        var card = new StoryProjectCard(project, [], 0);

        Assert.Equal(label, card.StatusLabel);
        Assert.Equal(tone, card.StatusTone);
        Assert.Empty(card.HistoryImages);
        Assert.Empty(card.HistoryVideos);
        Assert.False(card.HasHistoryImages);
        Assert.False(card.HasHistoryVideos);
    }

    private static StorySegmentDocument Segment() => new(
        "segment-1", "Shot", "Narrative", "Image", "Video", 4,
        null, null, null);
}
