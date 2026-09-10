using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IProjectConversationService
{
    Task<string> EnsureConversationAsync(
        WorkspaceProject project, WorkspaceContact contact, CancellationToken cancellationToken = default);
}
