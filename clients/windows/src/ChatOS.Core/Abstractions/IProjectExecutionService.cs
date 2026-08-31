using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IProjectExecutionService
{
    Task<ProjectRequirementExecutionLaunch?> FetchExecutionAsync(
        ProjectExecutionIdentity identity,
        CancellationToken cancellationToken = default);

    Task<ProjectExecutionActionResult> ConfirmExecutionAsync(
        ProjectExecutionIdentity identity,
        CancellationToken cancellationToken = default);

    Task<ProjectExecutionActionResult> StopExecutionAsync(
        ProjectExecutionIdentity identity,
        CancellationToken cancellationToken = default);
}
