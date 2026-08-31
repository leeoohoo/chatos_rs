using ChatOS.Connector.Connection;
using Microsoft.Extensions.Hosting;

namespace ChatOS.Connector.Runtime;

public sealed class ConnectorBackgroundService : BackgroundService
{
    private readonly ConnectorRuntimeContext _runtime;
    private readonly ConnectorConnectionStateMachine _state;
    private readonly ConnectorReconnectPolicy _reconnectPolicy;
    private readonly ConnectorSocketRequestFactory _requests;
    private readonly ConnectorGatewayConnection _connection;
    private readonly ConnectorPowerStateCoordinator _power;

    public ConnectorBackgroundService(
        ConnectorRuntimeContext runtime,
        ConnectorConnectionStateMachine state,
        ConnectorReconnectPolicy reconnectPolicy,
        ConnectorSocketRequestFactory requests,
        ConnectorGatewayConnection connection,
        ConnectorPowerStateCoordinator power)
    {
        _runtime = runtime;
        _state = state;
        _reconnectPolicy = reconnectPolicy;
        _requests = requests;
        _connection = connection;
        _power = power;
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        await _runtime.InitializeAsync(stoppingToken).ConfigureAwait(false);
        while (!stoppingToken.IsCancellationRequested)
        {
            var snapshot = _runtime.Snapshot;
            var powerSnapshot = _power.Snapshot;
            if (powerSnapshot.IsSuspended)
            {
                using var suspendedWait = CancellationTokenSource.CreateLinkedTokenSource(stoppingToken);
                var powerChange = _power.WaitForChangeAsync(powerSnapshot.Revision, suspendedWait.Token);
                var runtimeChange = _runtime.WaitForChangeAsync(
                    snapshot.ConnectionRevision,
                    Timeout.InfiniteTimeSpan,
                    suspendedWait.Token);
                await Task.WhenAny(powerChange, runtimeChange).ConfigureAwait(false);
                suspendedWait.Cancel();
                await ObserveAsync(powerChange).ConfigureAwait(false);
                await ObserveAsync(runtimeChange).ConfigureAwait(false);
                continue;
            }

            var configuration = await _runtime
                .SessionConfigurationAsync(stoppingToken)
                .ConfigureAwait(false);
            if (configuration is null)
            {
                if (_state.Snapshot.Phase is not ConnectorConnectionPhase.Unconfigured)
                {
                    _state.SetConfigured(false);
                }

                await _runtime.WaitForChangeAsync(
                    snapshot.ConnectionRevision,
                    TimeSpan.FromSeconds(5),
                    stoppingToken).ConfigureAwait(false);
                continue;
            }

            if (_state.Snapshot.Phase is ConnectorConnectionPhase.Unconfigured)
            {
                _state.SetConfigured(true);
            }

            ConnectorSocketRequest request;
            try
            {
                request = await _requests.CreateAsync(
                    configuration.GatewayBaseUri,
                    configuration.AccessToken,
                    configuration.DeviceId,
                    stoppingToken).ConfigureAwait(false);
            }
            catch when (!stoppingToken.IsCancellationRequested)
            {
                await WaitForRetryOrConfigurationChangeAsync(snapshot, stoppingToken)
                    .ConfigureAwait(false);
                continue;
            }

            using var connectionCancellation = CancellationTokenSource.CreateLinkedTokenSource(stoppingToken);
            var connectionTask = _connection.RunAsync(request, connectionCancellation.Token);
            var changeTask = _runtime.WaitForChangeAsync(
                snapshot.ConnectionRevision,
                Timeout.InfiniteTimeSpan,
                connectionCancellation.Token);
            var powerTask = _power.WaitForChangeAsync(
                powerSnapshot.Revision,
                connectionCancellation.Token);
            var completed = await Task.WhenAny(connectionTask, changeTask, powerTask).ConfigureAwait(false);
            if (completed != connectionTask)
            {
                connectionCancellation.Cancel();
                await ObserveAsync(connectionTask).ConfigureAwait(false);
                await ObserveAsync(changeTask).ConfigureAwait(false);
                await ObserveAsync(powerTask).ConfigureAwait(false);
                continue;
            }

            connectionCancellation.Cancel();
            await ObserveAsync(changeTask).ConfigureAwait(false);
            await ObserveAsync(powerTask).ConfigureAwait(false);
            await ObserveAsync(connectionTask).ConfigureAwait(false);
            await WaitForRetryOrConfigurationChangeAsync(snapshot, stoppingToken)
                .ConfigureAwait(false);
        }
    }

    private async Task WaitForRetryOrConfigurationChangeAsync(
        ConnectorRuntimeSnapshot snapshot,
        CancellationToken cancellationToken)
    {
        var delay = _reconnectPolicy.DelayAfterFailure(_state.Snapshot.ConsecutiveFailures);
        if (delay <= TimeSpan.Zero)
        {
            delay = TimeSpan.FromSeconds(1);
        }

        using var waitCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var delayTask = Task.Delay(delay, waitCancellation.Token);
        var changeTask = _runtime.WaitForChangeAsync(
            snapshot.ConnectionRevision,
            Timeout.InfiniteTimeSpan,
            waitCancellation.Token);
        var power = _power.Snapshot;
        var powerTask = _power.WaitForChangeAsync(power.Revision, waitCancellation.Token);
        await Task.WhenAny(delayTask, changeTask, powerTask).ConfigureAwait(false);
        waitCancellation.Cancel();
        await ObserveAsync(delayTask).ConfigureAwait(false);
        await ObserveAsync(changeTask).ConfigureAwait(false);
        await ObserveAsync(powerTask).ConfigureAwait(false);
    }

    private static async Task ObserveAsync(Task task)
    {
        try
        {
            await task.ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
        }
        catch
        {
            // Connection state contains the error and the outer loop owns reconnect timing.
        }
    }
}
