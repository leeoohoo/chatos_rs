namespace ChatOS.Core.Domain;

public sealed record StoryPlanningRequest(
    string ModelConfigId,
    string Title,
    string Description,
    string Source,
    string VisualStyle,
    string Ratio,
    int MaximumSegments = 200);

public sealed record PlannedStorySegment(
    string Title,
    string Narrative,
    string ImagePrompt,
    string VideoPrompt,
    int Seconds);

public sealed record StoryPlanningResult(
    string Summary,
    IReadOnlyList<PlannedStorySegment> Segments);
