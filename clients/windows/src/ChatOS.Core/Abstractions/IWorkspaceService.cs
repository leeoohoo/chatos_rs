using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IWorkspaceService
{
    Task<WorkspaceSnapshot> FetchWorkspaceAsync(CancellationToken cancellationToken = default);
}
