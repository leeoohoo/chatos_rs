using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IProjectRegistry
{
    Task<IReadOnlyList<LocalProjectRecord>> ListAsync(
        string ownerUserId, bool includeInactive = false, CancellationToken cancellationToken = default);
    Task<LocalProjectRecord?> GetAsync(string ownerUserId, string id, CancellationToken cancellationToken = default);
    Task<LocalProjectRecord> CreateAsync(
        string ownerUserId, LocalProjectDraft draft, CancellationToken cancellationToken = default);
    Task<LocalProjectRecord> UpdateAsync(
        string ownerUserId, string id, long expectedRevision, LocalProjectDraft draft,
        LocalProjectStatus status, CancellationToken cancellationToken = default);
}
