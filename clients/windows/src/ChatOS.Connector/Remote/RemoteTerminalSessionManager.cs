using System.Collections.Concurrent;
using System.Text;
using ChatOS.Connector.Terminal;

namespace ChatOS.Connector.Remote;

public sealed class RemoteTerminalSessionManager : IAsyncDisposable
{
    public const int MaximumSessions = 16;

    private readonly ConcurrentDictionary<string, Lazy<Task<IRemoteTerminalSession>>> _sessions =
        new(StringComparer.Ordinal);
    private readonly IRemoteTerminalSessionFactory _factory;
    private readonly SemaphoreSlim _registryGate = new(1, 1);

    public RemoteTerminalSessionManager(IRemoteTerminalSessionFactory factory)
    {
        _factory = factory;
    }

    public async Task<IRemoteTerminalSession> EnsureSessionAsync(
        RemoteTerminalSessionIdentity identity,
        TerminalSize size,
        string? verificationCode,
        CancellationToken cancellationToken = default)
    {
        ValidateIdentity(identity);
        await PruneExitedSessionsAsync(cancellationToken).ConfigureAwait(false);
        var replacementAttempted = false;
        while (true)
        {
            Lazy<Task<IRemoteTerminalSession>> lazy;
            await _registryGate.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                if (!_sessions.TryGetValue(identity.SessionId, out var existing))
                {
                    if (_sessions.Count >= MaximumSessions)
                    {
                        throw new InvalidOperationException(
                            $"Remote terminal session limit ({MaximumSessions}) has been reached.");
                    }
                    lazy = new Lazy<Task<IRemoteTerminalSession>>(
                        () => _factory.CreateAsync(
                            identity,
                            size,
                            verificationCode,
                            cancellationToken),
                        LazyThreadSafetyMode.ExecutionAndPublication);
                    _sessions[identity.SessionId] = lazy;
                }
                else
                {
                    lazy = existing;
                }
            }
            finally
            {
                _registryGate.Release();
            }
            IRemoteTerminalSession session;
            try
            {
                session = await lazy.Value.ConfigureAwait(false);
            }
            catch
            {
                _sessions.TryRemove(new KeyValuePair<string, Lazy<Task<IRemoteTerminalSession>>>(
                    identity.SessionId,
                    lazy));
                throw;
            }

            if (session.HasExited)
            {
                if (_sessions.TryRemove(new KeyValuePair<string, Lazy<Task<IRemoteTerminalSession>>>(
                        identity.SessionId,
                        lazy)))
                {
                    await session.DisposeAsync().ConfigureAwait(false);
                }
                if (replacementAttempted)
                {
                    throw new InvalidOperationException(
                        "Remote terminal session exited before it became ready.");
                }
                replacementAttempted = true;
                continue;
            }

            if (session.Identity != identity)
            {
                throw new InvalidOperationException(
                    "Remote terminal session id is already bound to another connection or workspace.");
            }

            await session.ResizeAsync(size, cancellationToken).ConfigureAwait(false);
            return session;
        }
    }

    public async Task<IRemoteTerminalSession?> GetAsync(string sessionId)
    {
        if (!_sessions.TryGetValue(sessionId, out var lazy))
        {
            return null;
        }

        try
        {
            var session = await lazy.Value.ConfigureAwait(false);
            if (!session.HasExited)
            {
                return session;
            }
            if (_sessions.TryRemove(new KeyValuePair<string, Lazy<Task<IRemoteTerminalSession>>>(
                    sessionId,
                    lazy)))
            {
                await session.DisposeAsync().ConfigureAwait(false);
            }
            return null;
        }
        catch
        {
            _sessions.TryRemove(new KeyValuePair<string, Lazy<Task<IRemoteTerminalSession>>>(
                sessionId,
                lazy));
            throw;
        }
    }

    public async Task<bool> CloseAsync(
        string sessionId,
        CancellationToken cancellationToken = default)
    {
        if (!_sessions.TryRemove(sessionId, out var lazy))
        {
            return false;
        }

        IRemoteTerminalSession? session = null;
        try
        {
            session = await lazy.Value.ConfigureAwait(false);
            await session.StopAsync(cancellationToken).ConfigureAwait(false);
            return true;
        }
        finally
        {
            if (session is not null)
            {
                await session.DisposeAsync().ConfigureAwait(false);
            }
        }
    }

    public async Task CloseAllAsync(CancellationToken cancellationToken = default)
        => await CloseWhereAsync(static _ => true, cancellationToken).ConfigureAwait(false);

    public async Task CloseConnectionAsync(
        string connectionId,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(connectionId))
        {
            return;
        }
        await CloseWhereAsync(
            session => string.Equals(
                session.Identity.ConnectionId,
                connectionId,
                StringComparison.Ordinal),
            cancellationToken).ConfigureAwait(false);
    }

    private async Task CloseWhereAsync(
        Func<IRemoteTerminalSession, bool> predicate,
        CancellationToken cancellationToken)
    {
        var sessions = _sessions.ToArray();
        foreach (var entry in sessions)
        {
            IRemoteTerminalSession? session = null;
            try
            {
                session = await entry.Value.Value.ConfigureAwait(false);
                if (!predicate(session) ||
                    !_sessions.TryRemove(new KeyValuePair<string, Lazy<Task<IRemoteTerminalSession>>>(
                        entry.Key,
                        entry.Value)))
                {
                    session = null;
                    continue;
                }
                await session.StopAsync(cancellationToken).ConfigureAwait(false);
            }
            catch
            {
                _sessions.TryRemove(new KeyValuePair<string, Lazy<Task<IRemoteTerminalSession>>>(
                    entry.Key,
                    entry.Value));
            }
            finally
            {
                if (session is not null)
                {
                    try
                    {
                        await session.DisposeAsync().ConfigureAwait(false);
                    }
                    catch
                    {
                    }
                }
            }
        }
    }

    public async ValueTask DisposeAsync() =>
        await CloseAllAsync(CancellationToken.None).ConfigureAwait(false);

    private static void ValidateIdentity(RemoteTerminalSessionIdentity identity)
    {
        if (string.IsNullOrWhiteSpace(identity.SessionId) ||
            Encoding.UTF8.GetByteCount(identity.SessionId) > 256 ||
            string.IsNullOrWhiteSpace(identity.WorkspaceId) ||
            string.IsNullOrWhiteSpace(identity.ConnectionId))
        {
            throw new ArgumentException("Remote terminal session identity is incomplete.", nameof(identity));
        }

        if (string.Equals(identity.SessionId, identity.ConnectionId, StringComparison.Ordinal))
        {
            throw new ArgumentException(
                "Remote terminal session id must be independent from the connection id.",
                nameof(identity));
        }
    }

    private async Task PruneExitedSessionsAsync(CancellationToken cancellationToken)
    {
        var removed = new List<IRemoteTerminalSession>();
        await _registryGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            foreach (var entry in _sessions.ToArray())
            {
                if (!entry.Value.IsValueCreated ||
                    !entry.Value.Value.IsCompletedSuccessfully ||
                    !entry.Value.Value.Result.HasExited ||
                    !_sessions.TryRemove(new KeyValuePair<string, Lazy<Task<IRemoteTerminalSession>>>(
                        entry.Key,
                        entry.Value)))
                {
                    continue;
                }
                removed.Add(entry.Value.Value.Result);
            }
        }
        finally
        {
            _registryGate.Release();
        }

        foreach (var session in removed)
        {
            await session.DisposeAsync().ConfigureAwait(false);
        }
    }
}
