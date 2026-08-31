using ChatOS.Connector.Relay;
using ChatOS.Connector.Terminal;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Tests;

public sealed class TerminalRelayHandlerTests
{
    [Fact]
    public async Task CreatesIdempotentSessionInsideAuthorizedProjectDirectory()
    {
        using var workspace = TestWorkspace.Create();
        Directory.CreateDirectory(Path.Combine(workspace.Root, "project"));
        var factory = new FakeFactory();
        await using var manager = new TerminalSessionManager(factory);
        var events = new ConnectorOutboundEventHub();
        var handler = new TerminalRelayHandler(workspace, manager, events);
        var dispatcher = Dispatcher(handler);

        var response = await dispatcher.DispatchAsync(CreatePayload(
            workspaceId: "workspace-1",
            headers: "\"x-local-connector-cwd\": \"project\""));

        Assert.Equal(200, response.Status);
        Assert.Equal("terminal_session_create_response", response.Type);
        var session = Assert.Single(factory.Sessions);
        Assert.Equal(Path.Combine(workspace.Root, "project"), session.Identity.WorkingDirectory);
        Assert.Equal("terminal-1", response.Body.GetProperty("terminal_session_id").GetString());
    }

    [Fact]
    public async Task InputResizeSnapshotAndCloseUseOneSessionIdentity()
    {
        using var workspace = TestWorkspace.Create();
        var factory = new FakeFactory();
        await using var manager = new TerminalSessionManager(factory);
        var events = new ConnectorOutboundEventHub();
        var handler = new TerminalRelayHandler(workspace, manager, events);
        var dispatcher = Dispatcher(handler);
        await dispatcher.DispatchAsync(CreatePayload("workspace-1", string.Empty));
        var session = Assert.Single(factory.Sessions);
        session.SnapshotValue = "terminal snapshot";

        Assert.True(await dispatcher.DispatchOneWayAsync(ControlPayload(
            "terminal_input",
            "workspace-1",
            "\"data\": \"dir\\r\\n\"")));
        Assert.Equal("dir\r\n", session.Written);
        Assert.True(await dispatcher.DispatchOneWayAsync(ControlPayload(
            "terminal_resize",
            "workspace-1",
            "\"cols\": 120, \"rows\": 40")));
        Assert.Equal(TerminalSize.Normalize(120, 40), session.LastSize);
        Assert.True(await dispatcher.DispatchOneWayAsync(ControlPayload(
            "terminal_snapshot_request",
            "workspace-1",
            "\"lines\": 200")));
        var snapshotEvent = await events.ReadAsync(CancellationToken.None);
        Assert.Contains("terminal_snapshot", snapshotEvent);
        Assert.Contains("terminal snapshot", snapshotEvent);

        Assert.True(await dispatcher.DispatchOneWayAsync(ControlPayload(
            "terminal_close",
            "workspace-1",
            string.Empty)));
        Assert.True(session.Stopped);
        Assert.True(session.Disposed);
    }

    [Fact]
    public async Task AnotherWorkspaceCannotControlOrCloseExistingSession()
    {
        using var workspace = TestWorkspace.Create();
        var factory = new FakeFactory();
        await using var manager = new TerminalSessionManager(factory);
        var events = new ConnectorOutboundEventHub();
        var handler = new TerminalRelayHandler(workspace, manager, events);
        var dispatcher = Dispatcher(handler);
        await dispatcher.DispatchAsync(CreatePayload("workspace-1", string.Empty));
        var session = Assert.Single(factory.Sessions);

        Assert.True(await dispatcher.DispatchOneWayAsync(ControlPayload(
            "terminal_close",
            "workspace-2",
            string.Empty)));

        Assert.False(session.Stopped);
        var errorEvent = await events.ReadAsync(CancellationToken.None);
        Assert.Contains("terminal_error", errorEvent);
    }

    private static RelayDispatcher Dispatcher(TerminalRelayHandler handler) =>
        new([handler], new AcceptingVerifier(), [handler]);

    private static string CreatePayload(string workspaceId, string headers) => $$"""
        {
          "type": "terminal_session_create_request",
          "request_id": "request-create",
          "owner_user_id": "owner-1",
          "device_id": "device-1",
          "workspace_id": "{{workspaceId}}",
          "headers": { {{headers}} },
          "body": {
            "terminal_session_id": "terminal-1",
            "cwd": ".",
            "cols": 80,
            "rows": 24
          }
        }
        """;

    private static string ControlPayload(string type, string workspaceId, string properties) => $$"""
        {
          "type": "{{type}}",
          "request_id": "request-control",
          "owner_user_id": "owner-1",
          "device_id": "device-1",
          "workspace_id": "{{workspaceId}}",
          "headers": {},
          "body": {
            "terminal_session_id": "terminal-1"{{(string.IsNullOrEmpty(properties) ? "" : ", " + properties)}}
          }
        }
        """;

    private sealed class AcceptingVerifier : IRelayRequestVerifier
    {
        public Task VerifyAsync(RelayRequest request, CancellationToken cancellationToken) =>
            Task.CompletedTask;
    }

    private sealed class FakeFactory : ITerminalSessionFactory
    {
        public List<FakeSession> Sessions { get; } = [];

        public Task<ITerminalSession> CreateAsync(
            TerminalSessionIdentity identity,
            TerminalSize size,
            CancellationToken cancellationToken = default)
        {
            var session = new FakeSession(identity) { LastSize = size };
            Sessions.Add(session);
            return Task.FromResult<ITerminalSession>(session);
        }
    }

    private sealed class FakeSession(TerminalSessionIdentity identity) : ITerminalSession
    {
        public TerminalSessionIdentity Identity { get; } = identity;

        public bool HasExited => false;

        public bool IsBusy => false;

        public string Written { get; private set; } = string.Empty;

        public TerminalSize LastSize { get; set; }

        public string SnapshotValue { get; set; } = string.Empty;

        public bool Stopped { get; private set; }

        public bool Disposed { get; private set; }

#pragma warning disable CS0067
        public event EventHandler<TerminalEvent>? EventReceived;
#pragma warning restore CS0067

        public Task WriteAsync(string data, CancellationToken cancellationToken = default)
        {
            Written += data;
            return Task.CompletedTask;
        }

        public Task ResizeAsync(TerminalSize size, CancellationToken cancellationToken = default)
        {
            LastSize = size;
            return Task.CompletedTask;
        }

        public string Snapshot(int maximumLines = 500) => SnapshotValue;

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

    private sealed class TestWorkspace : IConnectorWorkspaceCatalog, IDisposable
    {
        private TestWorkspace(string root)
        {
            Root = root;
        }

        public string Root { get; }

        public static TestWorkspace Create()
        {
            var root = Path.Combine(Path.GetTempPath(), $"chatos-terminal-{Guid.NewGuid():N}");
            Directory.CreateDirectory(root);
            return new TestWorkspace(root);
        }

        public ConnectorWorkspace? Find(string workspaceId) => workspaceId == "workspace-1"
            ? new ConnectorWorkspace("workspace-1", "Workspace", Root, "fingerprint")
            : null;

        public void Dispose() => Directory.Delete(Root, recursive: true);
    }
}
