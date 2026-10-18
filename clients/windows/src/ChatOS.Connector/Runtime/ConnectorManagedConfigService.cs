using System.Net;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.AgentTeams;
using ChatOS.Core.Abstractions;
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
    private static readonly TimeSpan CredentialRefreshCooldown = TimeSpan.FromMinutes(1);
    private readonly ConnectorRuntimeContext _runtime;
    private readonly ConnectorManagedConfigSynchronizer _synchronizer;
    private readonly ConnectorPairingService _pairing;
    private readonly ILocalConnectorPairingTicketService _pairingTickets;
    private readonly ILogger<ConnectorManagedConfigBackgroundService> _logger;
    private DateTimeOffset? _lastCredentialRefreshAttempt;

    public ConnectorManagedConfigBackgroundService(
        ConnectorRuntimeContext runtime,
        ConnectorManagedConfigSynchronizer synchronizer,
        ConnectorPairingService pairing,
        ILocalConnectorPairingTicketService pairingTickets,
        ILogger<ConnectorManagedConfigBackgroundService> logger)
    {
        _runtime = runtime;
        _synchronizer = synchronizer;
        _pairing = pairing;
        _pairingTickets = pairingTickets;
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
            catch (ConnectorGatewayException exception)
                when (exception.StatusCode == HttpStatusCode.Unauthorized)
            {
                if (await TryRefreshCredentialAsync(stoppingToken).ConfigureAwait(false))
                {
                    _logger.LogInformation(
                        "Local Connector credential was renewed after gateway rejection.");
                    continue;
                }

                _logger.LogWarning(
                    "Local Connector credential was rejected and could not be renewed yet.");
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

    private async Task<bool> TryRefreshCredentialAsync(CancellationToken cancellationToken)
    {
        var now = DateTimeOffset.UtcNow;
        if (_lastCredentialRefreshAttempt is { } lastAttempt &&
            now - lastAttempt < CredentialRefreshCooldown)
        {
            return false;
        }

        _lastCredentialRefreshAttempt = now;
        try
        {
            var ticket = await _pairingTickets.IssueAsync(cancellationToken).ConfigureAwait(false);
            _ = await _pairing.RefreshCredentialAsync(ticket, cancellationToken)
                .ConfigureAwait(false);
            return true;
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception exception)
        {
            _logger.LogWarning(
                "Unable to renew the rejected Local Connector credential. Failure type: {FailureType}.",
                exception.GetType().Name);
            return false;
        }
    }
}
