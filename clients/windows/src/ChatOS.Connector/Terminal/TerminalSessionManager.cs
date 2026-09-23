using System.Collections.Concurrent;
using System.Text;

namespace ChatOS.Connector.Terminal;

public sealed class TerminalSessionManager : IAsyncDisposable
{
    public const int MaximumSessions = 16;

    private readonly ConcurrentDictionary<string, Lazy<Task<ITerminalSession>>> _sessions =
        new(StringComparer.Ordinal);
    private readonly ITerminalSessionFactory _factory;
    private readonly SemaphoreSlim _registryGate = new(1, 1);

    public TerminalSessionManager(ITerminalSessionFactory factory)
    {
        _factory = factory;
    }

    public async Task<ITerminalSession> EnsureSessionAsync(
        TerminalSessionIdentity identity,
        TerminalSize size,
        CancellationToken cancellationToken = default)
    {
        ValidateIdentity(identity);
        await PruneExitedSessionsAsync(cancellationToken).ConfigureAwait(false);
        var replacementAttempted = false;
        while (true)
        {
            Lazy<Task<ITerminalSession>> lazy;
            await _registryGate.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                if (!_sessions.TryGetValue(identity.SessionId, out var existing))
                {
                    if (_sessions.Count >= MaximumSessions)
                    {
                        throw new InvalidOperationException(
                            $"Terminal session limit ({MaximumSessions}) has been reached.");
                    }
                    lazy = new Lazy<Task<ITerminalSession>>(
                        () => _factory.CreateAsync(identity, size, cancellationToken),
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
            ITerminalSession session;
            try
            {
                session = await lazy.Value.ConfigureAwait(false);
            }
            catch
            {
                _sessions.TryRemove(new KeyValuePair<string, Lazy<Task<ITerminalSession>>>(
                    identity.SessionId,
                    lazy));
                throw;
            }

            if (session.HasExited)
            {
                if (_sessions.TryRemove(new KeyValuePair<string, Lazy<Task<ITerminalSession>>>(
                        identity.SessionId,
                        lazy)))
                {
                    await session.DisposeAsync().ConfigureAwait(false);
                }

                if (replacementAttempted)
                {
                    throw new InvalidOperationException(
                        "Terminal session exited before it became ready.");
                }
                replacementAttempted = true;

                continue;
            }

            if (session.Identity != identity)
            {
                throw new InvalidOperationException(
                    "Terminal session id is already bound to a different workspace or directory.");
            }

            await session.ResizeAsync(size, cancellationToken).ConfigureAwait(false);
            return session;
        }
    }

    public async Task<ITerminalSession?> GetAsync(string sessionId)
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
            if (_sessions.TryRemove(new KeyValuePair<string, Lazy<Task<ITerminalSession>>>(
                    sessionId,
                    lazy)))
            {
                await session.DisposeAsync().ConfigureAwait(false);
            }
            return null;
        }
        catch
        {
            _sessions.TryRemove(new KeyValuePair<string, Lazy<Task<ITerminalSession>>>(
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

        ITerminalSession? session = null;
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

    public async ValueTask DisposeAsync()
    {
        await CloseAllAsync(CancellationToken.None).ConfigureAwait(false);
    }

    public async Task CloseAllAsync(CancellationToken cancellationToken = default)
        => await CloseWhereAsync(static _ => true, cancellationToken).ConfigureAwait(false);

    public async Task CloseRelaySessionsAsync(CancellationToken cancellationToken = default)
        => await CloseWhereAsync(
            static session => session.Identity.RelayOwned,
            cancellationToken).ConfigureAwait(false);

    private async Task CloseWhereAsync(
        Func<ITerminalSession, bool> predicate,
        CancellationToken cancellationToken)
    {
        var sessions = _sessions.ToArray();
        foreach (var entry in sessions)
        {
            ITerminalSession? session = null;
            try
            {
                session = await entry.Value.Value.ConfigureAwait(false);
                if (!predicate(session) ||
                    !_sessions.TryRemove(new KeyValuePair<string, Lazy<Task<ITerminalSession>>>(
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
                _sessions.TryRemove(new KeyValuePair<string, Lazy<Task<ITerminalSession>>>(
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

    private static void ValidateIdentity(TerminalSessionIdentity identity)
    {
        if (string.IsNullOrWhiteSpace(identity.SessionId) ||
            Encoding.UTF8.GetByteCount(identity.SessionId) > 256 ||
            string.IsNullOrWhiteSpace(identity.WorkspaceId) ||
            string.IsNullOrWhiteSpace(identity.WorkspaceRoot) ||
            string.IsNullOrWhiteSpace(identity.WorkingDirectory))
        {
            throw new ArgumentException("Terminal session identity is incomplete.", nameof(identity));
        }
    }

    private async Task PruneExitedSessionsAsync(CancellationToken cancellationToken)
    {
        var removed = new List<ITerminalSession>();
        await _registryGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            foreach (var entry in _sessions.ToArray())
            {
                if (!entry.Value.IsValueCreated ||
                    !entry.Value.Value.IsCompletedSuccessfully ||
                    !entry.Value.Value.Result.HasExited ||
                    !_sessions.TryRemove(new KeyValuePair<string, Lazy<Task<ITerminalSession>>>(
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
