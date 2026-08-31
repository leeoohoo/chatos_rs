using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IProjectPlanService
{
    Task<ProjectPlanSnapshot> FetchPlanAsync(
        string projectId,
        CancellationToken cancellationToken = default);

    Task<ProjectPlanSnapshot> FetchWorkItemsAsync(
        string projectId,
        string requirementId,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<ProjectRequirementDocument>> FetchDocumentsAsync(
        string projectId,
        string requirementId,
        CancellationToken cancellationToken = default);

    Task<ProjectRequirementExecutionLaunch?> FetchExecutionAsync(
        string projectId,
        string requirementId,
        CancellationToken cancellationToken = default);

    Task<ProjectRequirementExecutionLaunch> CreateExecutionAsync(
        string projectId,
        string requirementId,
        bool includePrerequisiteDependents,
        string? planningFeedback,
        CancellationToken cancellationToken = default);
}
