using ChatOS.Connector.Relay;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Runtime;

public sealed record ConnectorUser(
    string Id,
    string Username,
    string? DisplayName,
    string Role);

public sealed record ConnectorPersistentState(
    Uri GatewayBaseUri,
    ConnectorUser User,
    string DeviceId,
    string DeviceName,
    IReadOnlyList<ConnectorWorkspace> Workspaces,
    RemoteControlTrust RemoteControlTrust);

public sealed record ConnectorRuntimeSnapshot(
    long Revision,
    long ConnectionRevision,
    ConnectorPersistentState? State);

public sealed record ConnectorSessionConfiguration(
    Uri GatewayBaseUri,
    string AccessToken,
    string DeviceId);

public interface IConnectorPersistentStateStore
{
    Task<ConnectorPersistentState?> LoadAsync(CancellationToken cancellationToken = default);

    Task SaveAsync(
        ConnectorPersistentState? state,
        CancellationToken cancellationToken = default);
}
