using ChatOS.Connector.Relay;
using ChatOS.Connector.Remote;
using ChatOS.Connector.Terminal;
using ChatOS.Core.Domain;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Tests;

public sealed class RemoteTerminalRelayHandlerTests
{
    [Fact]
    public async Task CreatesV2SessionAndRoutesInputResizeSnapshotAndClose()
    {
        var factory = new FakeFactory();
        await using var manager = new RemoteTerminalSessionManager(factory);
        var events = new ConnectorOutboundEventHub();
        var handler = new RemoteTerminalRelayHandler(manager, events, new WorkspaceCatalog());
        var dispatcher = Dispatcher(handler);

        var response = await dispatcher.DispatchAsync(CreatePayload());

        Assert.Equal(200, response.Status);
        Assert.Equal("terminal_session_create_response", response.Type);
        Assert.Equal(2, response.Body.GetProperty("protocol_version").GetInt32());
        Assert.Equal("restored", response.Body.GetProperty("snapshot").GetString());
        Assert.Equal(7, response.Body.GetProperty("sequence").GetInt64());
        var session = Assert.Single(factory.Sessions);
        Assert.Equal("connection-1", session.Identity.ConnectionId);
        Assert.Equal("654321", factory.VerificationCode);

        Assert.True(await dispatcher.DispatchOneWayAsync(ControlPayload(
            "remote_terminal_input",
            "\"data\": \"ls\\r\"")));
        Assert.Equal("ls\r", session.Written);
        Assert.True(await dispatcher.DispatchOneWayAsync(ControlPayload(
            "remote_terminal_resize",
            "\"cols\": 132, \"rows\": 48")));
        Assert.Equal(TerminalSize.Normalize(132, 48), session.Size);
        Assert.True(await dispatcher.DispatchOneWayAsync(ControlPayload(
            "remote_terminal_snapshot_request",
            "\"lines\": 100")));
        var snapshotEvent = await events.ReadAsync(CancellationToken.None);
        Assert.Contains("\"base_sequence\":3", snapshotEvent);
        Assert.Contains("\"sequence\":7", snapshotEvent);

        Assert.True(await dispatcher.DispatchOneWayAsync(ControlPayload(
            "remote_terminal_close",
            string.Empty)));
        Assert.True(session.Stopped);
        Assert.True(session.Disposed);
    }

    [Fact]
    public async Task ReturnsStructuredSecondFactorChallenge()
    {
        var factory = new FakeFactory { RequireVerification = true };
        await using var manager = new RemoteTerminalSessionManager(factory);
        var handler = new RemoteTerminalRelayHandler(
            manager,
            new ConnectorOutboundEventHub(),
            new WorkspaceCatalog());

        var response = await Dispatcher(handler).DispatchAsync(CreatePayload(verificationCode: null));

        Assert.Equal(409, response.Status);
        Assert.Equal("second_factor_required", response.Body.GetProperty("code").GetString());
        Assert.Equal("OTP:", response.Body.GetProperty("prompt").GetString());
    }

    [Fact]
    public async Task AnotherWorkspaceCannotControlSession()
    {
        var factory = new FakeFactory();
        await using var manager = new RemoteTerminalSessionManager(factory);
        var events = new ConnectorOutboundEventHub();
        var handler = new RemoteTerminalRelayHandler(manager, events, new WorkspaceCatalog());
        var dispatcher = Dispatcher(handler);
        await dispatcher.DispatchAsync(CreatePayload());

        Assert.True(await dispatcher.DispatchOneWayAsync(ControlPayload(
            "remote_terminal_close",
            string.Empty,
            workspaceId: "workspace-2")));

        Assert.False(Assert.Single(factory.Sessions).Stopped);
        Assert.Contains("remote_terminal_not_found", await events.ReadAsync(CancellationToken.None));
    }

    private static RelayDispatcher Dispatcher(RemoteTerminalRelayHandler handler) =>
        new([handler], new AcceptingVerifier(), [handler]);

    private static string CreatePayload(string? verificationCode = "654321") => $$"""
        {
          "type": "remote_terminal_session_create_request",
          "request_id": "request-create",
          "owner_user_id": "owner-1",
          "device_id": "device-1",
          "workspace_id": "workspace-1",
          "headers": {},
          "body": {
            "terminal_session_id": "terminal-1",
            "connection": { "id": "connection-1", "password": "must-be-ignored" },
            "verification_code": {{(verificationCode is null ? "null" : "\"" + verificationCode + "\"")}},
            "cols": 80,
            "rows": 24
          }
        }
        """;

    private static string ControlPayload(
        string type,
        string properties,
        string workspaceId = "workspace-1") => $$"""
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

    private sealed class WorkspaceCatalog : IConnectorWorkspaceCatalog
    {
        public ConnectorWorkspace? Find(string workspaceId) =>
            workspaceId == "workspace-1"
                ? new ConnectorWorkspace("workspace-1", "Workspace", "C:\\workspace", "fingerprint")
                : null;
    }

    private sealed class FakeFactory : IRemoteTerminalSessionFactory
    {
        public List<FakeSession> Sessions { get; } = [];

        public bool RequireVerification { get; init; }

        public string? VerificationCode { get; private set; }

        public Task<IRemoteTerminalSession> CreateAsync(
            RemoteTerminalSessionIdentity identity,
            TerminalSize size,
            string? verificationCode,
            CancellationToken cancellationToken = default)
        {
            VerificationCode = verificationCode;
            if (RequireVerification && string.IsNullOrWhiteSpace(verificationCode))
            {
                throw new RemoteVerificationRequiredException("OTP:");
            }
            var session = new FakeSession(identity) { Size = size };
            Sessions.Add(session);
            return Task.FromResult<IRemoteTerminalSession>(session);
        }
    }

    private sealed class FakeSession(RemoteTerminalSessionIdentity identity) : IRemoteTerminalSession
    {
        public RemoteTerminalSessionIdentity Identity { get; } = identity;

        public bool HasExited => false;

        public string Written { get; private set; } = string.Empty;

        public TerminalSize Size { get; set; }

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
            Size = size;
            return Task.CompletedTask;
        }

        public TerminalSnapshot SnapshotState(int maximumLines = 500) =>
            new("restored", 3, 7, true);

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
