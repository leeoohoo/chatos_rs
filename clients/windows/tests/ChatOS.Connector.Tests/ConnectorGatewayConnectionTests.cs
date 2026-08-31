using System.Collections.Concurrent;
using System.Text.Json;
using System.Threading.Channels;
using ChatOS.Connector.Connection;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Approval;

namespace ChatOS.Connector.Tests;

public sealed class ConnectorGatewayConnectionTests
{
    [Fact]
    public async Task ReceivesPongWhileLongRelayWorkIsStillRunning()
    {
        var state = ConfiguredState();
        var socket = new FakeSocket();
        var handler = new BlockingHandler();
        var connection = Connection(state, socket, handler);
        using var cancellation = new CancellationTokenSource();
        var run = connection.RunAsync(Request(), cancellation.Token);

        socket.Receive("{\"type\":\"connected\"}");
        await WaitUntilAsync(() => state.Snapshot.Phase is ConnectorConnectionPhase.Connected);
        var connectedPong = state.Snapshot.LastPongAt;
        socket.Receive(RelayPayload());
        await handler.Started.Task.WaitAsync(TimeSpan.FromSeconds(2));
        await Task.Delay(10);
        socket.Receive("{\"type\":\"pong\"}");
        await WaitUntilAsync(() => state.Snapshot.LastPongAt > connectedPong);
        Assert.False(handler.Release.Task.IsCompleted);

        handler.Release.SetResult();
        await WaitUntilAsync(() => socket.Sent.Any(value => value.Contains("request-1", StringComparison.Ordinal)));
        cancellation.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => run);
        Assert.Equal(ConnectorConnectionPhase.Stopped, state.Snapshot.Phase);
    }

    [Fact]
    public async Task GatewayErrorMarksOnlyCurrentConnectionAsWaitingToReconnect()
    {
        var state = ConfiguredState();
        var socket = new FakeSocket();
        var connection = Connection(state, socket, new BlockingHandler(releaseImmediately: true));
        var run = connection.RunAsync(Request(), CancellationToken.None);

        socket.Receive("{\"type\":\"error\",\"message\":\"device rejected\"}");
        var error = await Assert.ThrowsAsync<IOException>(() => run);

        Assert.Contains("device rejected", error.Message);
        Assert.Equal(ConnectorConnectionPhase.WaitingToReconnect, state.Snapshot.Phase);
        Assert.Equal(1, state.Snapshot.ConsecutiveFailures);
    }

    [Fact]
    public async Task ConnectionSendsHeartbeatWithoutBlockingReceiveLoop()
    {
        var state = ConfiguredState();
        var socket = new FakeSocket();
        var connection = Connection(state, socket, new BlockingHandler(releaseImmediately: true));
        using var cancellation = new CancellationTokenSource();
        var run = connection.RunAsync(Request(), cancellation.Token);

        await WaitUntilAsync(() => socket.Sent.Contains("{\"type\":\"heartbeat\"}"));
        socket.Receive("{\"type\":\"connected\"}");
        await WaitUntilAsync(() => state.Snapshot.Phase is ConnectorConnectionPhase.Connected);

        cancellation.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => run);
    }

    [Fact]
    public async Task DisconnectDeniesPendingApprovalsAndClearsSessionAllowlist()
    {
        var state = ConfiguredState();
        var socket = new FakeSocket();
        var approvals = new CommandApprovalCoordinator(
            new CommandApprovalCoordinatorTests.MemoryApprovalStore());
        var pending = approvals.RequestAsync(new CommandApprovalRequest(
            "request-pending",
            "owner-1",
            "device-1",
            "workspace-1",
            "git",
            ["status"],
            "C:\\workspace",
            "test",
            "scope"), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.Low));
        await WaitUntilAsync(() => approvals.Snapshot().Count == 1);
        var dispatcher = new RelayDispatcher(
            [new BlockingHandler(releaseImmediately: true)],
            new AcceptingVerifier());
        var connection = new ConnectorGatewayConnection(
            state,
            new FakeSocketFactory(socket),
            dispatcher,
            heartbeatInterval: TimeSpan.FromSeconds(30),
            approvals: approvals);
        using var cancellation = new CancellationTokenSource();
        var run = connection.RunAsync(Request(), cancellation.Token);

        cancellation.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => run);

        Assert.False((await pending).Approved);
        Assert.Empty(approvals.Snapshot());
    }

    private static ConnectorGatewayConnection Connection(
        ConnectorConnectionStateMachine state,
        FakeSocket socket,
        IRelayRequestHandler handler)
    {
        var dispatcher = new RelayDispatcher([handler], new AcceptingVerifier());
        return new ConnectorGatewayConnection(
            state,
            new FakeSocketFactory(socket),
            dispatcher,
            heartbeatInterval: TimeSpan.FromSeconds(30));
    }

    private static ConnectorConnectionStateMachine ConfiguredState()
    {
        var state = new ConnectorConnectionStateMachine();
        state.SetConfigured(true);
        return state;
    }

    private static ConnectorSocketRequest Request() => new(
        new Uri("wss://gateway.example/connect"),
        new Dictionary<string, string>());

    private static string RelayPayload() => """
        {
          "type": "workspace_filesystem_request",
          "request_id": "request-1",
          "owner_user_id": "owner-1",
          "device_id": "device-1",
          "workspace_id": "workspace-1",
          "headers": {},
          "body": { "operation": "list" }
        }
        """;

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!condition())
        {
            await Task.Delay(5, timeout.Token);
        }
    }

    private sealed class FakeSocketFactory(FakeSocket socket) : IConnectorSocketFactory
    {
        public Task<IConnectorSocket> ConnectAsync(
            ConnectorSocketRequest request,
            CancellationToken cancellationToken) =>
            Task.FromResult<IConnectorSocket>(socket);
    }

    private sealed class FakeSocket : IConnectorSocket
    {
        private readonly Channel<string?> _received = Channel.CreateUnbounded<string?>();

        public ConcurrentBag<string> Sent { get; } = [];

        public void Receive(string payload) => _received.Writer.TryWrite(payload);

        public Task SendTextAsync(string payload, CancellationToken cancellationToken)
        {
            Sent.Add(payload);
            return Task.CompletedTask;
        }

        public async Task<string?> ReceiveTextAsync(CancellationToken cancellationToken) =>
            await _received.Reader.ReadAsync(cancellationToken);

        public Task CloseAsync(CancellationToken cancellationToken) => Task.CompletedTask;

        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }

    private sealed class AcceptingVerifier : IRelayRequestVerifier
    {
        public Task VerifyAsync(RelayRequest request, CancellationToken cancellationToken) =>
            Task.CompletedTask;
    }

    private sealed class BlockingHandler : IRelayRequestHandler
    {
        public BlockingHandler(bool releaseImmediately = false)
        {
            if (releaseImmediately)
            {
                Release.SetResult();
            }
        }

        public TaskCompletionSource Started { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource Release { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public bool CanHandle(string requestType) => requestType == "workspace_filesystem_request";

        public string ResponseType(string requestType) => "workspace_filesystem_response";

        public async Task<RelayHandlerResult> HandleAsync(
            RelayRequest request,
            CancellationToken cancellationToken)
        {
            Started.TrySetResult();
            await Release.Task.WaitAsync(cancellationToken);
            return RelayHandlerResult.Ok(JsonSerializer.SerializeToElement(new { ok = true }));
        }
    }
}
