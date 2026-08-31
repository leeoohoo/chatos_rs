using ChatOS.Connector.Connection;
using ChatOS.Connector.Gateway;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Runtime;

public sealed class LocalConnectorControlService : ILocalConnectorControlService
{
    private readonly ConnectorRuntimeContext _runtime;
    private readonly ConnectorPairingService _pairing;
    private readonly ConnectorConnectionStateMachine _connection;
    private readonly IConnectorGatewayClient _gateway;
    private readonly IConnectorAccessTokenStore _tokens;

    public LocalConnectorControlService(
        ConnectorRuntimeContext runtime,
        ConnectorPairingService pairing,
        ConnectorConnectionStateMachine connection,
        IConnectorGatewayClient gateway,
        IConnectorAccessTokenStore tokens)
    {
        _runtime = runtime;
        _pairing = pairing;
        _connection = connection;
        _gateway = gateway;
        _tokens = tokens;
    }

    public async Task<LocalConnectorStatus> GetStatusAsync(CancellationToken cancellationToken = default)
    {
        await _runtime.InitializeAsync(cancellationToken).ConfigureAwait(false);
        return Status();
    }

    public async Task<LocalConnectorStatus> PairAsync(
        LocalConnectorPairingDraft draft,
        string ticket,
        CancellationToken cancellationToken = default)
    {
        await _runtime.InitializeAsync(cancellationToken).ConfigureAwait(false);
        if (!Uri.TryCreate(draft.GatewayBaseUrl.Trim(), UriKind.Absolute, out var gateway) ||
            gateway.Scheme is not ("http" or "https"))
        {
            throw new ArgumentException("Connector Gateway 必须是有效的 HTTP(S) 地址。");
        }

        _ = await _pairing.PairAsync(
            new ConnectorPairingRequest(
                gateway,
                ticket,
                draft.DeviceName,
                draft.Workspaces.Select(static workspace =>
                    new ConnectorWorkspacePairing(workspace.AbsoluteRoot, workspace.Alias)).ToArray()),
            cancellationToken).ConfigureAwait(false);
        _connection.SetConfigured(true);
        return Status();
    }

    public async Task DisconnectAsync(CancellationToken cancellationToken = default)
    {
        await _runtime.InitializeAsync(cancellationToken).ConfigureAwait(false);
        var state = _runtime.Snapshot.State;
        var token = await _tokens.GetAccessTokenAsync(cancellationToken).ConfigureAwait(false);
        Exception? remoteError = null;
        if (state is not null && !string.IsNullOrWhiteSpace(token))
        {
            try
            {
                await _gateway.DisconnectDeviceAsync(
                    state.GatewayBaseUri,
                    token,
                    state.DeviceId,
                    cancellationToken).ConfigureAwait(false);
            }
            catch (Exception exception) when (exception is not OperationCanceledException)
            {
                remoteError = exception;
            }
        }

        await _runtime.ReplaceAsync(null, CancellationToken.None).ConfigureAwait(false);
        await _tokens.ClearAsync(CancellationToken.None).ConfigureAwait(false);
        _connection.SetConfigured(false);
        if (remoteError is not null)
        {
            throw new InvalidOperationException(
                "本机配对已清除，但网关暂时未确认设备断开。服务器会在连接过期后清理状态。",
                remoteError);
        }
    }

    private LocalConnectorStatus Status()
    {
        var state = _runtime.Snapshot.State;
        var connection = _connection.Snapshot;
        return new LocalConnectorStatus(
            state is not null,
            connection.Phase.ToString(),
            state?.User.Username,
            state?.DeviceId,
            state?.DeviceName,
            state?.GatewayBaseUri.ToString(),
            connection.ConnectedAt,
            connection.LastPongAt,
            connection.LastError,
            state?.Workspaces.Select(static workspace => new LocalConnectorWorkspaceStatus(
                workspace.Id,
                workspace.Alias,
                workspace.AbsoluteRoot)).ToArray()
                ?? Array.Empty<LocalConnectorWorkspaceStatus>());
    }
}
