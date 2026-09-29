using ChatOS.Connector.Gateway;
using ChatOS.Connector.AgentTeams;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

namespace ChatOS.Connector.Runtime;

public sealed class ConnectorManagedConfigSynchronizer
{
    private readonly ConnectorRuntimeContext _runtime;
    private readonly IConnectorGatewayClient _gateway;
    private readonly AgentTeamRuntimeSettingsProvider _agentRuntimeSettings;

    public ConnectorManagedConfigSynchronizer(
        ConnectorRuntimeContext runtime,
        IConnectorGatewayClient gateway,
        AgentTeamRuntimeSettingsProvider agentRuntimeSettings)
    {
        _runtime = runtime;
        _gateway = gateway;
        _agentRuntimeSettings = agentRuntimeSettings;
    }

    public async Task<bool> SyncAsync(CancellationToken cancellationToken = default)
    {
        var state = _runtime.Snapshot.State;
        var session = await _runtime.SessionConfigurationAsync(cancellationToken).ConfigureAwait(false);
        if (state is null || session is null)
        {
            return false;
        }

        var managed = await _gateway.GetManagedRuntimeConfigAsync(
            session.GatewayBaseUri,
            session.AccessToken,
            cancellationToken).ConfigureAwait(false);
        var trustChanged = await _runtime.UpdateRemoteControlTrustAsync(
            state.GatewayBaseUri,
            state.DeviceId,
            managed.RemoteControlTrust,
            cancellationToken).ConfigureAwait(false);
        var runtimeChanged = _agentRuntimeSettings.Update(managed.NativeAgentRuntimeSettings);
        return trustChanged || runtimeChanged;
    }
}

public sealed class ConnectorManagedConfigBackgroundService : BackgroundService
{
    private readonly ConnectorRuntimeContext _runtime;
    private readonly ConnectorManagedConfigSynchronizer _synchronizer;
    private readonly ILogger<ConnectorManagedConfigBackgroundService> _logger;

    public ConnectorManagedConfigBackgroundService(
        ConnectorRuntimeContext runtime,
        ConnectorManagedConfigSynchronizer synchronizer,
        ILogger<ConnectorManagedConfigBackgroundService> logger)
    {
        _runtime = runtime;
        _synchronizer = synchronizer;
        _logger = logger;
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        await _runtime.InitializeAsync(stoppingToken).ConfigureAwait(false);
        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                if (await _synchronizer.SyncAsync(stoppingToken).ConfigureAwait(false))
                {
                    _logger.LogInformation("Local Connector managed trust configuration was refreshed.");
                }
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
            catch (Exception exception)
            {
                _logger.LogWarning(
                    "Unable to refresh Local Connector managed trust configuration; keeping the persisted snapshot. Failure type: {FailureType}.",
                    exception.GetType().Name);
            }

            await Task.Delay(TimeSpan.FromSeconds(60), stoppingToken).ConfigureAwait(false);
        }
    }
}
