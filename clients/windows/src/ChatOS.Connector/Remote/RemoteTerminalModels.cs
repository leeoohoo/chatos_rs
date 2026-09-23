using ChatOS.Connector.Terminal;

namespace ChatOS.Connector.Remote;

public sealed record RemoteTerminalSessionIdentity(
    string SessionId,
    string WorkspaceId,
    string ConnectionId);

public interface IRemoteTerminalSession : IAsyncDisposable
{
    RemoteTerminalSessionIdentity Identity { get; }

    bool HasExited { get; }

    event EventHandler<TerminalEvent>? EventReceived;

    Task WriteAsync(string data, CancellationToken cancellationToken = default);

    Task ResizeAsync(TerminalSize size, CancellationToken cancellationToken = default);

    TerminalSnapshot SnapshotState(int maximumLines = 500);

    Task StopAsync(CancellationToken cancellationToken = default);
}

public interface IRemoteTerminalSessionFactory
{
    Task<IRemoteTerminalSession> CreateAsync(
        RemoteTerminalSessionIdentity identity,
        TerminalSize size,
        string? verificationCode,
        CancellationToken cancellationToken = default);
}
