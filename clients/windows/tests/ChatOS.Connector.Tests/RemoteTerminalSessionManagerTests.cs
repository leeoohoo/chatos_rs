using ChatOS.Connector.Remote;
using ChatOS.Connector.Terminal;

namespace ChatOS.Connector.Tests;

public sealed class RemoteTerminalSessionManagerTests
{
    [Fact]
    public async Task ConcurrentEnsureCreatesOneSessionAndKeepsConnectionBinding()
    {
        var factory = new FakeFactory();
        await using var manager = new RemoteTerminalSessionManager(factory);
        var identity = Identity();

        var sessions = await Task.WhenAll(Enumerable.Range(0, 10).Select(_ =>
            manager.EnsureSessionAsync(identity, TerminalSize.Normalize(80, 24), null)));

        Assert.Equal(1, factory.CreateCount);
        Assert.All(sessions, session => Assert.Same(sessions[0], session));

        var error = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            manager.EnsureSessionAsync(
                identity with { ConnectionId = "connection-2" },
                TerminalSize.Normalize(80, 24),
                null));
        Assert.Contains("another connection", error.Message);
    }

    [Fact]
    public async Task CloseStopsAndDisposesSession()
    {
        var factory = new FakeFactory();
        await using var manager = new RemoteTerminalSessionManager(factory);
        var session = (FakeSession)await manager.EnsureSessionAsync(
            Identity(),
            TerminalSize.Normalize(80, 24),
            null);

        Assert.True(await manager.CloseAsync(Identity().SessionId));
        Assert.True(session.Stopped);
        Assert.True(session.Disposed);
        Assert.Null(await manager.GetAsync(Identity().SessionId));
    }

    [Fact]
    public async Task SessionIdCannotReuseConnectionId()
    {
        await using var manager = new RemoteTerminalSessionManager(new FakeFactory());
        var error = await Assert.ThrowsAsync<ArgumentException>(() =>
            manager.EnsureSessionAsync(
                new RemoteTerminalSessionIdentity("same", "workspace-1", "same"),
                TerminalSize.Normalize(80, 24),
                null));

        Assert.Contains("independent", error.Message);
    }

    [Fact]
    public async Task ClosingConnectionOnlyStopsItsTerminalTabs()
    {
        var factory = new FakeFactory();
        await using var manager = new RemoteTerminalSessionManager(factory);
        var first = (FakeSession)await manager.EnsureSessionAsync(
            Identity(),
            TerminalSize.Normalize(80, 24),
            null);
        var secondIdentity = new RemoteTerminalSessionIdentity(
            "terminal-2",
            "workspace-1",
            "connection-2");
        var second = (FakeSession)await manager.EnsureSessionAsync(
            secondIdentity,
            TerminalSize.Normalize(80, 24),
            null);

        await manager.CloseConnectionAsync("connection-1");

        Assert.True(first.Stopped);
        Assert.True(first.Disposed);
        Assert.Null(await manager.GetAsync(first.Identity.SessionId));
        Assert.False(second.Stopped);
        Assert.Same(second, await manager.GetAsync(second.Identity.SessionId));
    }

    private static RemoteTerminalSessionIdentity Identity() =>
        new("terminal-1", "workspace-1", "connection-1");

    private sealed class FakeFactory : IRemoteTerminalSessionFactory
    {
        public int CreateCount { get; private set; }

        public Task<IRemoteTerminalSession> CreateAsync(
            RemoteTerminalSessionIdentity identity,
            TerminalSize size,
            string? verificationCode,
            CancellationToken cancellationToken = default)
        {
            CreateCount++;
            return Task.FromResult<IRemoteTerminalSession>(new FakeSession(identity));
        }
    }

    private sealed class FakeSession(RemoteTerminalSessionIdentity identity) : IRemoteTerminalSession
    {
        public RemoteTerminalSessionIdentity Identity { get; } = identity;

        public bool HasExited => false;

        public bool Stopped { get; private set; }

        public bool Disposed { get; private set; }

#pragma warning disable CS0067
        public event EventHandler<TerminalEvent>? EventReceived;
#pragma warning restore CS0067

        public Task WriteAsync(string data, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task ResizeAsync(TerminalSize size, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public TerminalSnapshot SnapshotState(int maximumLines = 500) =>
            new(string.Empty, 0, 0, false);

        public Task StopAsync(CancellationToken cancellationToken = default)
        {
            Stopped = true;
            return Task.CompletedTask;
        }

        public ValueTask DisposeAsync()
        {
            Disposed = true;
            return ValueTask.CompletedTask;
        }
    }
}
