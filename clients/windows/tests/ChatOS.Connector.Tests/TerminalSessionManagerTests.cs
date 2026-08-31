using ChatOS.Connector.Terminal;

namespace ChatOS.Connector.Tests;

public sealed class TerminalSessionManagerTests
{
    [Fact]
    public void OutputBufferKeepsBoundedTailAndLineSnapshot()
    {
        var buffer = new TerminalOutputBuffer(maximumCharacters: 12);
        buffer.Append("line1\nline2\nline3");

        Assert.Equal("line2\nline3", buffer.Snapshot(2));
        Assert.True(buffer.Snapshot(500).Length <= 12);
    }

    [Fact]
    public async Task ConcurrentEnsureCreatesOneNativeSession()
    {
        var factory = new FakeFactory();
        await using var manager = new TerminalSessionManager(factory);
        var identity = Identity();

        var sessions = await Task.WhenAll(Enumerable.Range(0, 10).Select(_ =>
            manager.EnsureSessionAsync(identity, TerminalSize.Normalize(80, 24))));

        Assert.Equal(1, factory.CreateCount);
        Assert.All(sessions, session => Assert.Same(sessions[0], session));
        Assert.Equal(10, ((FakeSession)sessions[0]).ResizeCount);
    }

    [Fact]
    public async Task ExistingSessionIdCannotSwitchWorkspaceIdentity()
    {
        await using var manager = new TerminalSessionManager(new FakeFactory());
        await manager.EnsureSessionAsync(Identity(), TerminalSize.Normalize(80, 24));
        var other = Identity() with { WorkspaceId = "workspace-2" };

        var error = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            manager.EnsureSessionAsync(other, TerminalSize.Normalize(80, 24)));

        Assert.Contains("different workspace", error.Message);
    }

    [Fact]
    public async Task ExitedSessionIsReplacedAndCloseKillsAndDisposesIt()
    {
        var factory = new FakeFactory();
        await using var manager = new TerminalSessionManager(factory);
        var first = (FakeSession)await manager.EnsureSessionAsync(
            Identity(),
            TerminalSize.Normalize(80, 24));
        first.HasExitedValue = true;

        var second = (FakeSession)await manager.EnsureSessionAsync(
            Identity(),
            TerminalSize.Normalize(100, 30));
        Assert.NotSame(first, second);
        Assert.True(first.Disposed);

        Assert.True(await manager.CloseAsync(Identity().SessionId));
        Assert.True(second.Stopped);
        Assert.True(second.Disposed);
        Assert.False(await manager.CloseAsync(Identity().SessionId));
    }

    [Theory]
    [InlineData(0, 0, 1, 1)]
    [InlineData(5000, 5000, 1000, 1000)]
    [InlineData(120, 40, 120, 40)]
    public void TerminalSizeIsBounded(int columns, int rows, int expectedColumns, int expectedRows)
    {
        var size = TerminalSize.Normalize(columns, rows);

        Assert.Equal(expectedColumns, size.Columns);
        Assert.Equal(expectedRows, size.Rows);
    }

    private static TerminalSessionIdentity Identity() => new(
        "session-1",
        "workspace-1",
        "C:\\workspace",
        "C:\\workspace\\project");

    private sealed class FakeFactory : ITerminalSessionFactory
    {
        public int CreateCount { get; private set; }

        public Task<ITerminalSession> CreateAsync(
            TerminalSessionIdentity identity,
            TerminalSize size,
            CancellationToken cancellationToken = default)
        {
            CreateCount++;
            return Task.FromResult<ITerminalSession>(new FakeSession(identity));
        }
    }

    private sealed class FakeSession(TerminalSessionIdentity identity) : ITerminalSession
    {
        public TerminalSessionIdentity Identity { get; } = identity;

        public bool HasExitedValue { get; set; }

        public bool HasExited => HasExitedValue;

        public bool IsBusy => false;

        public int ResizeCount { get; private set; }

        public bool Stopped { get; private set; }

        public bool Disposed { get; private set; }

#pragma warning disable CS0067
        public event EventHandler<TerminalEvent>? EventReceived;
#pragma warning restore CS0067

        public Task WriteAsync(string data, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task ResizeAsync(TerminalSize size, CancellationToken cancellationToken = default)
        {
            ResizeCount++;
            return Task.CompletedTask;
        }

        public string Snapshot(int maximumLines = 500) => string.Empty;

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
