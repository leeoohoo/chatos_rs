using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Remote;

/// Closes live SSH terminal channels before a saved connection and its local
/// credentials are removed. Other connection operations remain delegated to
/// the existing Windows runtime.
public sealed class TerminalAwareRemoteConnectionService : IRemoteConnectionService
{
    private readonly WindowsRemoteConnectionService _inner;
    private readonly RemoteTerminalSessionManager _terminals;

    public TerminalAwareRemoteConnectionService(
        WindowsRemoteConnectionService inner,
        RemoteTerminalSessionManager terminals)
    {
        _inner = inner;
        _terminals = terminals;
    }

    public Task<IReadOnlyList<RemoteConnection>> ListAsync(
        CancellationToken cancellationToken = default) =>
        _inner.ListAsync(cancellationToken);

    public Task<RemoteConnection> CreateAsync(
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default) =>
        _inner.CreateAsync(draft, cancellationToken);

    public Task<RemoteConnection> UpdateAsync(
        string id,
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default) =>
        _inner.UpdateAsync(id, draft, cancellationToken);

    public async Task DeleteAsync(
        string id,
        CancellationToken cancellationToken = default)
    {
        await _terminals.CloseConnectionAsync(id, cancellationToken).ConfigureAwait(false);
        await _inner.DeleteAsync(id, cancellationToken).ConfigureAwait(false);
    }

    public Task<RemoteConnectionTestResult> TestDraftAsync(
        RemoteConnectionDraft draft,
        string? verificationCode,
        CancellationToken cancellationToken = default) =>
        _inner.TestDraftAsync(draft, verificationCode, cancellationToken);

    public Task<RemoteConnectionTestResult> TestSavedAsync(
        string id,
        string? verificationCode,
        CancellationToken cancellationToken = default) =>
        _inner.TestSavedAsync(id, verificationCode, cancellationToken);
}
