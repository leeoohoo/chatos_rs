using ChatOS.Core.State;

namespace ChatOS.Core.Abstractions;

public interface IWorkspaceRelationsService
{
    // No project CRUD/list requests. Projects come exclusively from the local registry.
    Task<WorkspaceRelationsSnapshot> FetchWorkspaceRelationsAsync(CancellationToken cancellationToken = default);
}
