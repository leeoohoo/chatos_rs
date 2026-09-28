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
    string Kind,
    string Title,
    string Narrative,
    string ImagePrompt,
    string VideoPrompt,
    int Seconds,
    IReadOnlyList<string> ResourceIds);

public sealed record PlannedStoryResource(
    string Id,
    string Kind,
    string Name,
    string Description,
    string ImagePrompt);

public sealed record StoryPlanningResult(
    string Summary,
    IReadOnlyList<PlannedStoryResource> Resources,
    IReadOnlyList<PlannedStorySegment> Segments);

public enum StoryOptimizationTarget
{
    Source,
    VisualStyle,
}

public sealed record StoryOptimizationRequest(
    string ModelConfigId,
    string Title,
    string Description,
    string Source,
    string VisualStyle,
    StoryOptimizationTarget Target);

public sealed record StoryOptimizationSuggestion(
    string OptimizedText,
    string Rationale);
