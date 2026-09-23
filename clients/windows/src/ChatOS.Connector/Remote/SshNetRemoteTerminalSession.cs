using System.Text;
using ChatOS.Connector.Terminal;
using Renci.SshNet;

namespace ChatOS.Connector.Remote;

internal interface IRemoteTerminalChannel : IAsyncDisposable
{
    ValueTask<int> ReadAsync(Memory<byte> buffer, CancellationToken cancellationToken);

    ValueTask WriteAsync(ReadOnlyMemory<byte> buffer, CancellationToken cancellationToken);

    Task FlushAsync(CancellationToken cancellationToken);

    void Resize(TerminalSize size);
}

internal sealed class SshNetRemoteTerminalChannel(
    ShellStream stream,
    RemoteSshSession owner) : IRemoteTerminalChannel
{
    private int _disposed;

    public ValueTask<int> ReadAsync(Memory<byte> buffer, CancellationToken cancellationToken) =>
        stream.ReadAsync(buffer, cancellationToken);

    public ValueTask WriteAsync(ReadOnlyMemory<byte> buffer, CancellationToken cancellationToken) =>
        stream.WriteAsync(buffer, cancellationToken);

    public Task FlushAsync(CancellationToken cancellationToken) =>
        stream.FlushAsync(cancellationToken);

    public void Resize(TerminalSize size) =>
        stream.ChangeWindowSize(size.Columns, size.Rows, 0, 0);

    public ValueTask DisposeAsync()
    {
        if (Interlocked.Exchange(ref _disposed, 1) == 0)
        {
            stream.Dispose();
            owner.Dispose();
        }
        return ValueTask.CompletedTask;
    }
}

public sealed class SshNetRemoteTerminalSessionFactory : IRemoteTerminalSessionFactory
{
    private readonly IRemoteConnectionRuntime _runtime;
    private readonly IRemoteSshSessionFactory _sshSessions;
    private readonly ConnectorOutboundEventHub _events;

    public SshNetRemoteTerminalSessionFactory(
        IRemoteConnectionRuntime runtime,
        IRemoteSshSessionFactory sshSessions,
        ConnectorOutboundEventHub events)
    {
        _runtime = runtime;
        _sshSessions = sshSessions;
        _events = events;
    }

    public async Task<IRemoteTerminalSession> CreateAsync(
        RemoteTerminalSessionIdentity identity,
        TerminalSize size,
        string? verificationCode,
        CancellationToken cancellationToken = default)
    {
        var draft = await _runtime.ResolveDraftAsync(identity.ConnectionId, cancellationToken)
            .ConfigureAwait(false);
        if (!string.Equals(
                draft.LocalConnectorWorkspaceId,
                identity.WorkspaceId,
                StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "Remote connection is not assigned to this connector workspace.");
        }

        var ssh = await _sshSessions.ConnectAsync(draft, verificationCode, cancellationToken)
            .ConfigureAwait(false);
        try
        {
            var stream = ssh.TargetClient.CreateShellStream(
                "xterm-256color",
                size.Columns,
                size.Rows,
                0,
                0,
                64 * 1024);
            if (StartupCommand(draft.DefaultRemotePath) is { } startupCommand)
            {
                var startupBytes = Encoding.UTF8.GetBytes(startupCommand + "\n");
                await stream.WriteAsync(startupBytes, cancellationToken).ConfigureAwait(false);
                await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
            }
            var session = new SshNetRemoteTerminalSession(
                identity,
                new SshNetRemoteTerminalChannel(stream, ssh));
            session.EventReceived += (_, value) => _events.Publish(value);
            session.Start();
            return session;
        }
        catch
        {
            ssh.Dispose();
            throw;
        }
    }

    internal static string? StartupCommand(string? rawDirectory)
    {
        var directory = string.IsNullOrWhiteSpace(rawDirectory) ? "~" : rawDirectory.Trim();
        if (directory == "~")
        {
            return null;
        }

        var target = directory.StartsWith("~/", StringComparison.Ordinal)
            ? $"\"$HOME\"/{Quote(directory[2..])}"
            : Quote(directory);
        return $"cd -- {target}";
    }

    private static string Quote(string value) =>
        "'" + value.Replace("'", "'\\''", StringComparison.Ordinal) + "'";
}

internal sealed class SshNetRemoteTerminalSession : IRemoteTerminalSession
{
    private readonly TerminalOutputBuffer _output = new();
    private readonly IRemoteTerminalChannel _channel;
    private readonly SemaphoreSlim _writeGate = new(1, 1);
    private readonly CancellationTokenSource _lifetime = new();
    private Task? _readerTask;
    private int _exited;
    private int _stopping;
    private int _disposed;

    internal SshNetRemoteTerminalSession(
        RemoteTerminalSessionIdentity identity,
        IRemoteTerminalChannel channel)
    {
        Identity = identity;
        _channel = channel;
    }

    public RemoteTerminalSessionIdentity Identity { get; }

    public bool HasExited => Volatile.Read(ref _exited) != 0;

    public event EventHandler<TerminalEvent>? EventReceived;

    internal void Start()
    {
        if (_readerTask is not null)
        {
            throw new InvalidOperationException("Remote terminal session was already started.");
        }
        Publish(new TerminalEvent(
            TerminalEventKind.State,
            Identity.SessionId,
            Busy: false,
            Remote: true,
            State: "ready"));
        _readerTask = ReadOutputAsync(_lifetime.Token);
    }

    public async Task WriteAsync(string data, CancellationToken cancellationToken = default)
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref _disposed) != 0, this);
        TerminalTransportChunker.ValidateInput(data);
        if (HasExited)
        {
            throw new InvalidOperationException("Remote terminal session has exited.");
        }

        var bytes = Encoding.UTF8.GetBytes(data);
        await _writeGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _channel.WriteAsync(bytes, cancellationToken).ConfigureAwait(false);
            await _channel.FlushAsync(cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            _writeGate.Release();
        }
    }

    public Task ResizeAsync(TerminalSize size, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        ObjectDisposedException.ThrowIf(Volatile.Read(ref _disposed) != 0, this);
        if (!HasExited)
        {
            _channel.Resize(size);
        }
        return Task.CompletedTask;
    }

    public TerminalSnapshot SnapshotState(int maximumLines = 500) =>
        _output.SnapshotState(maximumLines);

    public async Task StopAsync(CancellationToken cancellationToken = default)
    {
        if (Interlocked.Exchange(ref _stopping, 1) != 0)
        {
            return;
        }

        Publish(new TerminalEvent(
            TerminalEventKind.State,
            Identity.SessionId,
            Busy: false,
            Remote: true,
            State: "closed"));
        _lifetime.Cancel();
        await _channel.DisposeAsync().ConfigureAwait(false);
        if (_readerTask is not null)
        {
            try
            {
                await _readerTask.WaitAsync(TimeSpan.FromSeconds(3), cancellationToken)
                    .ConfigureAwait(false);
            }
            catch (TimeoutException)
            {
            }
        }
        Interlocked.Exchange(ref _exited, 1);
    }

    public async ValueTask DisposeAsync()
    {
        if (Interlocked.Exchange(ref _disposed, 1) != 0)
        {
            return;
        }

        try
        {
            await StopAsync(CancellationToken.None).ConfigureAwait(false);
        }
        catch
        {
        }
        _lifetime.Dispose();
        _writeGate.Dispose();
    }

    private async Task ReadOutputAsync(CancellationToken cancellationToken)
    {
        var bytes = new byte[16 * 1024];
        var decoder = Encoding.UTF8.GetDecoder();
        var characters = new char[Encoding.UTF8.GetMaxCharCount(bytes.Length)];
        try
        {
            while (!cancellationToken.IsCancellationRequested)
            {
                var read = await _channel.ReadAsync(bytes, cancellationToken).ConfigureAwait(false);
                if (read == 0)
                {
                    break;
                }

                var count = decoder.GetChars(bytes, 0, read, characters, 0, flush: false);
                PublishOutput(new string(characters, 0, count));
            }

            var remaining = decoder.GetChars(
                Array.Empty<byte>(),
                0,
                0,
                characters,
                0,
                flush: true);
            PublishOutput(new string(characters, 0, remaining));
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (ObjectDisposedException) when (Volatile.Read(ref _stopping) != 0)
        {
        }
        catch (Exception exception)
        {
            Publish(new TerminalEvent(
                TerminalEventKind.Error,
                Identity.SessionId,
                Data: exception.Message,
                Remote: true,
                ErrorCode: "ssh_terminal_read_failed",
                Recoverable: false));
        }
        finally
        {
            Interlocked.Exchange(ref _exited, 1);
            try
            {
                await _channel.DisposeAsync().ConfigureAwait(false);
            }
            catch
            {
            }
            if (Volatile.Read(ref _stopping) == 0)
            {
                Publish(new TerminalEvent(
                    TerminalEventKind.Exit,
                    Identity.SessionId,
                    ExitCode: null,
                    Busy: false,
                    Remote: true));
            }
        }
    }

    private void PublishOutput(string text)
    {
        foreach (var chunk in TerminalTransportChunker.SplitOutput(text))
        {
            var sequence = _output.Append(chunk);
            Publish(new TerminalEvent(
                TerminalEventKind.Output,
                Identity.SessionId,
                Data: chunk,
                Sequence: sequence,
                Remote: true));
        }
    }

    private void Publish(TerminalEvent value)
    {
        var handlers = EventReceived;
        if (handlers is null)
        {
            return;
        }

        foreach (EventHandler<TerminalEvent> handler in handlers.GetInvocationList())
        {
            try
            {
                handler(this, value);
            }
            catch
            {
                // A gateway subscriber cannot terminate the SSH reader.
            }
        }
    }
}
