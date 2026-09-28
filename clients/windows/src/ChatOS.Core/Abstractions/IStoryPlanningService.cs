using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IStoryPlanningService
{
    Task<StoryPlanningResult> PlanAsync(
        StoryPlanningRequest request,
        CancellationToken cancellationToken = default);
}
