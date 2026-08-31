using ChatOS.Connector.Relay;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Runtime;

public sealed class ConnectorRuntimeContext :
    IConnectorWorkspaceContext,
    IRelaySecurityContextProvider
{
    private readonly object _gate = new();
    private readonly SemaphoreSlim _mutationGate = new(1, 1);
    private readonly IConnectorPersistentStateStore _store;
    private readonly IConnectorAccessTokenStore _tokens;
    private ConnectorRuntimeSnapshot _snapshot = new(0, 0, null);
    private TaskCompletionSource<long> _changed = ChangeSource();
    private bool _initialized;

    public ConnectorRuntimeContext(
        IConnectorPersistentStateStore store,
        IConnectorAccessTokenStore tokens)
    {
        _store = store;
        _tokens = tokens;
    }

    public ConnectorRuntimeSnapshot Snapshot
    {
        get
        {
            lock (_gate)
            {
                return _snapshot;
            }
        }
    }

    public string? DeviceId => Snapshot.State?.DeviceId;

    public IReadOnlyList<ConnectorWorkspace> Workspaces =>
        Snapshot.State?.Workspaces ?? Array.Empty<ConnectorWorkspace>();

    public async Task InitializeAsync(CancellationToken cancellationToken = default)
    {
        await _mutationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_initialized)
            {
                return;
            }

            var state = await _store.LoadAsync(cancellationToken).ConfigureAwait(false);
            lock (_gate)
            {
                _snapshot = new ConnectorRuntimeSnapshot(
                    _snapshot.Revision + 1,
                    _snapshot.ConnectionRevision + 1,
                    state);
                _initialized = true;
                SignalChanged();
            }
        }
        finally
        {
            _mutationGate.Release();
        }
    }

    public async Task ReplaceAsync(
        ConnectorPersistentState? state,
        CancellationToken cancellationToken = default)
    {
        await _mutationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _store.SaveAsync(state, cancellationToken).ConfigureAwait(false);
            lock (_gate)
            {
                _snapshot = new ConnectorRuntimeSnapshot(
                    _snapshot.Revision + 1,
                    _snapshot.ConnectionRevision + 1,
                    state);
                _initialized = true;
                SignalChanged();
            }
        }
        finally
        {
            _mutationGate.Release();
        }
    }

    public async Task<bool> UpdateRemoteControlTrustAsync(
        Uri expectedGatewayBaseUri,
        string expectedDeviceId,
        RemoteControlTrust trust,
        CancellationToken cancellationToken = default)
    {
        await _mutationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var current = Snapshot.State;
            if (current is null ||
                current.GatewayBaseUri != expectedGatewayBaseUri ||
                !string.Equals(current.DeviceId, expectedDeviceId, StringComparison.Ordinal))
            {
                return false;
            }

            if (TrustEquals(current.RemoteControlTrust, trust))
            {
                return false;
            }

            var next = current with { RemoteControlTrust = trust };
            await _store.SaveAsync(next, cancellationToken).ConfigureAwait(false);
            lock (_gate)
            {
                // Trust is read for every Relay verification; refreshing it does not restart the socket.
                _snapshot = new ConnectorRuntimeSnapshot(
                    _snapshot.Revision + 1,
                    _snapshot.ConnectionRevision,
                    next);
            }

            return true;
        }
        finally
        {
            _mutationGate.Release();
        }
    }

    public ConnectorWorkspace? Find(string workspaceId)
    {
        var state = Snapshot.State;
        return state?.Workspaces.FirstOrDefault(workspace =>
            string.Equals(workspace.Id, workspaceId, StringComparison.Ordinal));
    }

    public Task<RelaySecurityContext> GetAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var state = Snapshot.State
            ?? throw new RelayRequestException(403, "Local connector is not paired.");
        return Task.FromResult(new RelaySecurityContext(
            state.User.Id,
            state.DeviceId,
            state.RemoteControlTrust));
    }

    public async Task<ConnectorSessionConfiguration?> SessionConfigurationAsync(
        CancellationToken cancellationToken = default)
    {
        var state = Snapshot.State;
        if (state is null || string.IsNullOrWhiteSpace(state.DeviceId))
        {
            return null;
        }

        var token = await _tokens.GetAccessTokenAsync(cancellationToken).ConfigureAwait(false);
        return string.IsNullOrWhiteSpace(token)
            ? null
            : new ConnectorSessionConfiguration(state.GatewayBaseUri, token, state.DeviceId);
    }

    public async Task<long> WaitForChangeAsync(
        long afterConnectionRevision,
        TimeSpan maximumWait,
        CancellationToken cancellationToken)
    {
        Task<long> changeTask;
        lock (_gate)
        {
            if (_snapshot.ConnectionRevision > afterConnectionRevision)
            {
                return _snapshot.ConnectionRevision;
            }

            changeTask = _changed.Task;
        }

        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(maximumWait);
        try
        {
            return await changeTask.WaitAsync(timeout.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            return Snapshot.ConnectionRevision;
        }
    }

    private void SignalChanged()
    {
        var previous = _changed;
        _changed = ChangeSource();
        previous.TrySetResult(_snapshot.ConnectionRevision);
    }

    private static TaskCompletionSource<long> ChangeSource() =>
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    private static bool TrustEquals(RemoteControlTrust left, RemoteControlTrust right) =>
        left.RequireSignedMessages == right.RequireSignedMessages &&
        left.SignatureMaxSkewSeconds == right.SignatureMaxSkewSeconds &&
        left.TrustedRelayPublicKeys.Count == right.TrustedRelayPublicKeys.Count &&
        left.TrustedRelayPublicKeys.All(pair =>
            right.TrustedRelayPublicKeys.TryGetValue(pair.Key, out var value) &&
            string.Equals(pair.Value, value, StringComparison.Ordinal));
}
