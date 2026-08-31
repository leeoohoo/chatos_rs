using ChatOS.Connector.Gateway;

namespace ChatOS.Connector.Runtime;

public interface IConnectorControlledNetworkReadinessService
{
    Task<ConnectorControlledNetworkReadiness> CheckAsync(
        CancellationToken cancellationToken = default);
}

public sealed class ConnectorControlledNetworkReadinessService(
    ConnectorRuntimeContext runtime,
    IConnectorGatewayClient gateway) : IConnectorControlledNetworkReadinessService
{
    public async Task<ConnectorControlledNetworkReadiness> CheckAsync(
        CancellationToken cancellationToken = default)
    {
        var state = runtime.Snapshot.State;
        var session = await runtime.SessionConfigurationAsync(cancellationToken).ConfigureAwait(false);
        if (state is null || session is null)
        {
            return new ConnectorControlledNetworkReadiness(
                false,
                "connector_not_paired",
                null,
                0);
        }

        return await gateway.GetControlledNetworkReadinessAsync(
            session.GatewayBaseUri,
            session.AccessToken,
            state.DeviceId,
            cancellationToken).ConfigureAwait(false);
    }
}
