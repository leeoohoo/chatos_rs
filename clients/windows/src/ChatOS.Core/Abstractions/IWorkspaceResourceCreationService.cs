using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IWorkspaceResourceCreationService
{
    Task<WorkspaceProject> CreateLocalProjectAsync(
        LocalProjectCreationDraft draft,
        CancellationToken cancellationToken = default);

    Task BindContactAsync(
        string projectId,
        string contactId,
        CancellationToken cancellationToken = default);

    Task<string> EnsureConversationAsync(
        WorkspaceProject project,
        WorkspaceContact contact,
        CancellationToken cancellationToken = default);
}
