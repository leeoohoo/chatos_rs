using System.Collections.Concurrent;
using System.Text.Json;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Terminal;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Connection;

public sealed class ConnectorGatewayConnection
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private static readonly HashSet<string> RelayRequestTypes =
    [
        "terminal_exec_request",
        "terminal_session_create_request",
        "workspace_directory_list_request",
        "workspace_directory_create_request",
        "workspace_filesystem_request",
        "mcp",
        "plugin_prepare_request",
        "plugin_execute_request",
        "plugin_cancel_request",
    ];
    private static readonly HashSet<string> OneWayRelayTypes =
    [
        "terminal_input",
        "terminal_command",
        "terminal_resize",
        "terminal_snapshot_request",
        "terminal_close",
    ];

    private readonly ConnectorConnectionStateMachine _state;
    private readonly IConnectorSocketFactory _socketFactory;
    private readonly RelayDispatcher _relayDispatcher;
    private readonly TimeProvider _timeProvider;
    private readonly TimeSpan _heartbeatInterval;
    private readonly ConnectorOutboundEventHub? _outboundEvents;
    private readonly TerminalSessionManager? _terminalSessions;
    private readonly CommandApprovalCoordinator? _approvals;
    private readonly IPluginRuntimeLifetime? _pluginSessions;

    public ConnectorGatewayConnection(
        ConnectorConnectionStateMachine state,
        IConnectorSocketFactory socketFactory,
        RelayDispatcher relayDispatcher,
        TimeProvider? timeProvider = null,
        TimeSpan? heartbeatInterval = null,
        ConnectorOutboundEventHub? outboundEvents = null,
        TerminalSessionManager? terminalSessions = null,
        CommandApprovalCoordinator? approvals = null,
        IPluginRuntimeLifetime? pluginSessions = null)
    {
        _state = state;
        _socketFactory = socketFactory;
        _relayDispatcher = relayDispatcher;
        _timeProvider = timeProvider ?? TimeProvider.System;
        _heartbeatInterval = heartbeatInterval ?? TimeSpan.FromSeconds(15);
        _outboundEvents = outboundEvents;
        _terminalSessions = terminalSessions;
        _approvals = approvals;
        _pluginSessions = pluginSessions;
    }

    public async Task RunAsync(
        ConnectorSocketRequest request,
        CancellationToken cancellationToken)
    {
        var lease = _state.Start();
        try
        {
            await using var socket = await _socketFactory
                .ConnectAsync(request, cancellationToken)
                .ConfigureAwait(false);
            using var sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            var heartbeat = new ConnectorHeartbeatMonitor();
            heartbeat.Reset(_timeProvider.GetUtcNow());
            _outboundEvents?.Drain();
            var relayTasks = new ConcurrentDictionary<Guid, Task>();
            using var relayConcurrency = new SemaphoreSlim(8, 8);

            var receiveTask = ReceiveLoopAsync(
                socket,
                lease,
                heartbeat,
                relayTasks,
                relayConcurrency,
                sessionCancellation.Token);
            var heartbeatTask = HeartbeatLoopAsync(
                socket,
                heartbeat,
                sessionCancellation.Token);
            var outboundTask = OutboundEventLoopAsync(socket, sessionCancellation.Token);
            var completed = await Task.WhenAny(receiveTask, heartbeatTask, outboundTask).ConfigureAwait(false);
            sessionCancellation.Cancel();
            try
            {
                await completed.ConfigureAwait(false);
            }
            finally
            {
                try
                {
                    await socket.CloseAsync(CancellationToken.None).ConfigureAwait(false);
                }
                catch
                {
                    // The transport is already considered failed; close is best effort.
                }

                if (_terminalSessions is not null)
                {
                    await _terminalSessions.CloseAllAsync(CancellationToken.None).ConfigureAwait(false);
                }

                if (_approvals is not null)
                {
                    await _approvals.CancelAllAsync(
                        "The local connector disconnected before approval completed.",
                        CancellationToken.None).ConfigureAwait(false);
                }

                if (_pluginSessions is not null)
                {
                    await _pluginSessions.TerminateAllAsync().ConfigureAwait(false);
                }

                _outboundEvents?.Drain();
                await ObserveShutdownAsync(receiveTask, heartbeatTask, outboundTask).ConfigureAwait(false);
                await AwaitRelayTasksAsync(relayTasks.Values).ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            _state.Stop(lease);
            throw;
        }
        catch (Exception exception)
        {
            _state.MarkFailed(lease, exception.Message);
            throw;
        }
    }

    private async Task ReceiveLoopAsync(
        IConnectorSocket socket,
        ConnectorConnectionLease lease,
        ConnectorHeartbeatMonitor heartbeat,
        ConcurrentDictionary<Guid, Task> relayTasks,
        SemaphoreSlim relayConcurrency,
        CancellationToken cancellationToken)
    {
        while (!cancellationToken.IsCancellationRequested)
        {
            var payload = await socket.ReceiveTextAsync(cancellationToken).ConfigureAwait(false);
            if (payload is null)
            {
                throw new IOException("Connector gateway closed the WebSocket connection.");
            }

            if (string.IsNullOrWhiteSpace(payload))
            {
                continue;
            }

            string? messageType;
            try
            {
                using var document = JsonDocument.Parse(payload);
                messageType = document.RootElement.TryGetProperty("type", out var type) &&
                    type.ValueKind is JsonValueKind.String
                        ? type.GetString()
                        : null;
            }
            catch (JsonException)
            {
                continue;
            }

            switch (messageType)
            {
                case "connected":
                    var connectedAt = _timeProvider.GetUtcNow();
                    _state.MarkConnected(lease, connectedAt);
                    heartbeat.Reset(connectedAt);
                    break;
                case "pong":
                    var pongAt = _timeProvider.GetUtcNow();
                    _state.MarkPong(lease, pongAt);
                    heartbeat.RecordPong(pongAt);
                    break;
                case "error":
                    throw new IOException(ReadGatewayError(payload));
                default:
                    if (messageType is not null && OneWayRelayTypes.Contains(messageType))
                    {
                        TrackRelayTask(
                            relayTasks,
                            _relayDispatcher.DispatchOneWayAsync(payload, cancellationToken));
                    }
                    else if (messageType is not null && RelayRequestTypes.Contains(messageType))
                    {
                        TrackRelayTask(
                            relayTasks,
                            DispatchAndSendAsync(
                                socket,
                                payload,
                                relayConcurrency,
                                cancellationToken));
                    }
                    break;
            }
        }
    }

    private async Task OutboundEventLoopAsync(
        IConnectorSocket socket,
        CancellationToken cancellationToken)
    {
        if (_outboundEvents is null)
        {
            await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken).ConfigureAwait(false);
            return;
        }

        while (!cancellationToken.IsCancellationRequested)
        {
            var payload = await _outboundEvents.ReadAsync(cancellationToken).ConfigureAwait(false);
            await socket.SendTextAsync(payload, cancellationToken).ConfigureAwait(false);
        }
    }

    private async Task HeartbeatLoopAsync(
        IConnectorSocket socket,
        ConnectorHeartbeatMonitor heartbeat,
        CancellationToken cancellationToken)
    {
        while (!cancellationToken.IsCancellationRequested)
        {
            var sentAt = _timeProvider.GetUtcNow();
            await socket.SendTextAsync("{\"type\":\"heartbeat\"}", cancellationToken)
                .ConfigureAwait(false);
            await Task.Delay(_heartbeatInterval, _timeProvider, cancellationToken).ConfigureAwait(false);
            if (heartbeat.CompleteHeartbeat(sentAt))
            {
                throw new TimeoutException("Connector gateway missed three heartbeat acknowledgements.");
            }
        }
    }

    private async Task DispatchAndSendAsync(
        IConnectorSocket socket,
        string payload,
        SemaphoreSlim relayConcurrency,
        CancellationToken cancellationToken)
    {
        await relayConcurrency.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var response = await _relayDispatcher
                .DispatchAsync(payload, cancellationToken)
                .ConfigureAwait(false);
            var responsePayload = JsonSerializer.Serialize(response, JsonOptions);
            await socket.SendTextAsync(responsePayload, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            relayConcurrency.Release();
        }
    }

    private static void TrackRelayTask(
        ConcurrentDictionary<Guid, Task> relayTasks,
        Task task)
    {
        var id = Guid.NewGuid();
        relayTasks[id] = task;
        _ = task.ContinueWith(
            completedTask => relayTasks.TryRemove(id, out _),
            CancellationToken.None,
            TaskContinuationOptions.ExecuteSynchronously,
            TaskScheduler.Default);
    }

    private static async Task ObserveShutdownAsync(params Task[] tasks)
    {
        try
        {
            await Task.WhenAll(tasks).ConfigureAwait(false);
        }
        catch
        {
            // The first connection failure is propagated by RunAsync.
        }
    }

    private static async Task AwaitRelayTasksAsync(IEnumerable<Task> tasks)
    {
        try
        {
            await Task.WhenAll(tasks.ToArray()).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            // Session shutdown cancels in-flight Relay work.
        }
        catch
        {
            // Each Relay request is isolated from the connection lifecycle.
        }
    }

    private static string ReadGatewayError(string payload)
    {
        using var document = JsonDocument.Parse(payload);
        foreach (var name in new[] { "message", "code" })
        {
            if (document.RootElement.TryGetProperty(name, out var value) &&
                value.ValueKind is JsonValueKind.String &&
                !string.IsNullOrWhiteSpace(value.GetString()))
            {
                return value.GetString()!;
            }
        }

        return "Connector gateway reported an error.";
    }
}
