using ChatOS.Connector.Connection;

namespace ChatOS.Connector.Runtime;

public sealed record ConnectorPowerSnapshot(bool IsSuspended, long Revision);

public sealed class ConnectorPowerStateCoordinator
{
    private readonly object _gate = new();
    private readonly ConnectorConnectionStateMachine _connectionState;
    private ConnectorPowerSnapshot _snapshot = new(false, 0);
    private TaskCompletionSource<long> _changed = ChangeSource();

    public ConnectorPowerStateCoordinator(ConnectorConnectionStateMachine connectionState)
    {
        _connectionState = connectionState;
    }

    public ConnectorPowerSnapshot Snapshot
    {
        get
        {
            lock (_gate) return _snapshot;
        }
    }

    public void Suspend()
    {
        lock (_gate)
        {
            if (_snapshot.IsSuspended) return;
            _connectionState.Suspend();
            _snapshot = new ConnectorPowerSnapshot(true, _snapshot.Revision + 1);
            SignalChanged();
        }
    }

    public void Resume()
    {
        lock (_gate)
        {
            if (!_snapshot.IsSuspended) return;
            _snapshot = new ConnectorPowerSnapshot(false, _snapshot.Revision + 1);
            SignalChanged();
        }
    }

    public Task<long> WaitForChangeAsync(long afterRevision, CancellationToken cancellationToken)
    {
        lock (_gate)
        {
            if (_snapshot.Revision > afterRevision) return Task.FromResult(_snapshot.Revision);
            return _changed.Task.WaitAsync(cancellationToken);
        }
    }

    private void SignalChanged()
    {
        var previous = _changed;
        _changed = ChangeSource();
        previous.TrySetResult(_snapshot.Revision);
    }

    private static TaskCompletionSource<long> ChangeSource() =>
        new(TaskCreationOptions.RunContinuationsAsynchronously);
}
