using System.Collections.Concurrent;

namespace ChatOS.Connector.Terminal;

public sealed class TerminalSessionManager : IAsyncDisposable
{
    private readonly ConcurrentDictionary<string, Lazy<Task<ITerminalSession>>> _sessions =
        new(StringComparer.Ordinal);
    private readonly ITerminalSessionFactory _factory;

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
        while (true)
        {
            var lazy = _sessions.GetOrAdd(
                identity.SessionId,
                _ => new Lazy<Task<ITerminalSession>>(
                    () => _factory.CreateAsync(identity, size, cancellationToken),
                    LazyThreadSafetyMode.ExecutionAndPublication));
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
            return session.HasExited ? null : session;
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
    {
        var sessions = _sessions.ToArray();
        _sessions.Clear();
        foreach (var entry in sessions)
        {
            ITerminalSession? session = null;
            try
            {
                session = await entry.Value.Value.ConfigureAwait(false);
                await session.StopAsync(cancellationToken).ConfigureAwait(false);
            }
            catch
            {
            }
            finally
            {
                if (session is not null)
                {
                    await session.DisposeAsync().ConfigureAwait(false);
                }
            }
        }
    }

    private static void ValidateIdentity(TerminalSessionIdentity identity)
    {
        if (string.IsNullOrWhiteSpace(identity.SessionId) ||
            string.IsNullOrWhiteSpace(identity.WorkspaceId) ||
            string.IsNullOrWhiteSpace(identity.WorkspaceRoot) ||
            string.IsNullOrWhiteSpace(identity.WorkingDirectory))
        {
            throw new ArgumentException("Terminal session identity is incomplete.", nameof(identity));
        }
    }
}
