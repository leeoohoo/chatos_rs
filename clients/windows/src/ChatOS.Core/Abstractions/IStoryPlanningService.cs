using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IStoryPlanningService
{
    Task<StoryPlanningResult> PlanAsync(
        StoryPlanningRequest request,
        CancellationToken cancellationToken = default);

    Task<StoryOptimizationSuggestion> OptimizeAsync(
        StoryOptimizationRequest request,
        CancellationToken cancellationToken = default);

    Task<StorySegmentRefinementSuggestion> RefineSegmentAsync(
        StorySegmentRefinementRequest request,
        CancellationToken cancellationToken = default);
}
