using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

// Host operations only. No remote project CRUD, Git import or conversation prerequisite.
public interface ILocalProjectsService
{
    Task<string?> GetDeviceIdAsync(string ownerUserId, CancellationToken cancellationToken = default);
    Task<LocalProjectRecord> CreateAsync(string ownerUserId, LocalProjectDraft draft, string expectedWorkspaceRoot, CancellationToken cancellationToken = default);
    Task<LocalProjectRecord> RenameAsync(string ownerUserId, string projectId, long expectedRevision, string name, CancellationToken cancellationToken = default);
    Task RemoveAsync(string ownerUserId, string projectId, long expectedRevision, CancellationToken cancellationToken = default);
    Task<ProjectContextSnapshot> ResolveContextAsync(string ownerUserId, string projectId, CancellationToken cancellationToken = default);
}
