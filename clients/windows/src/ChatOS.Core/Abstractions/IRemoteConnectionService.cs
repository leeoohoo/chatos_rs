using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IRemoteConnectionCloudService
{
    Task<IReadOnlyList<RemoteConnection>> ListAsync(CancellationToken cancellationToken = default);

    Task<RemoteConnection> CreateAsync(
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default);

    Task<RemoteConnection> UpdateAsync(
        string id,
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default);

    Task DeleteAsync(string id, CancellationToken cancellationToken = default);
}

public interface IRemoteConnectionService : IRemoteConnectionCloudService
{
    Task<RemoteConnectionTestResult> TestDraftAsync(
        RemoteConnectionDraft draft,
        string? verificationCode,
        CancellationToken cancellationToken = default);

    Task<RemoteConnectionTestResult> TestSavedAsync(
        string id,
        string? verificationCode,
        CancellationToken cancellationToken = default);
}
