using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Remote;

/// <summary>
/// Keeps the desktop shell startable when the optional local-agent host is not installed.
/// Remote connection metadata becomes available as soon as the local agent is configured.
/// </summary>
public sealed class UnavailableRemoteConnectionMetadataService : IRemoteConnectionMetadataService
{
    public Task<IReadOnlyList<RemoteConnection>> ListAsync(
        CancellationToken cancellationToken = default) =>
        Task.FromResult<IReadOnlyList<RemoteConnection>>(Array.Empty<RemoteConnection>());

    public Task<RemoteConnection> CreateAsync(
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default) =>
        throw Unavailable();

    public Task<RemoteConnection> UpdateAsync(
        string id,
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default) =>
        throw Unavailable();

    public Task DeleteAsync(string id, CancellationToken cancellationToken = default) =>
        throw Unavailable();

    private static InvalidOperationException Unavailable() =>
        new("Remote connections require the optional local agent host.");
}
