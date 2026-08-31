namespace ChatOS.Connector.Connection;

public sealed class ConnectorConnectionStateMachine
{
    private readonly object _gate = new();
    private ConnectorConnectionSnapshot _snapshot = new(
        ConnectorConnectionPhase.Unconfigured,
        0,
        0,
        null,
        null,
        null);

    public ConnectorConnectionSnapshot Snapshot
    {
        get
        {
            lock (_gate)
            {
                return _snapshot;
            }
        }
    }

    public void SetConfigured(bool configured)
    {
        lock (_gate)
        {
            var nextGeneration = _snapshot.Generation + 1;
            _snapshot = configured
                ? new(ConnectorConnectionPhase.Stopped, nextGeneration, 0, null, null, null)
                : new(ConnectorConnectionPhase.Unconfigured, nextGeneration, 0, null, null, null);
        }
    }

    public ConnectorConnectionLease Start()
    {
        lock (_gate)
        {
            if (_snapshot.Phase is ConnectorConnectionPhase.Unconfigured)
            {
                throw new InvalidOperationException("The local connector is not configured.");
            }

            var generation = _snapshot.Generation + 1;
            _snapshot = new(
                ConnectorConnectionPhase.Connecting,
                generation,
                _snapshot.ConsecutiveFailures,
                null,
                null,
                null);
            return new(generation);
        }
    }

    public bool MarkConnected(ConnectorConnectionLease lease, DateTimeOffset connectedAt)
    {
        lock (_gate)
        {
            if (!IsCurrent(lease) || _snapshot.Phase is not ConnectorConnectionPhase.Connecting)
            {
                return false;
            }

            _snapshot = _snapshot with
            {
                Phase = ConnectorConnectionPhase.Connected,
                ConsecutiveFailures = 0,
                ConnectedAt = connectedAt,
                LastPongAt = connectedAt,
                LastError = null,
            };
            return true;
        }
    }

    public bool MarkPong(ConnectorConnectionLease lease, DateTimeOffset receivedAt)
    {
        lock (_gate)
        {
            if (!IsCurrent(lease) || _snapshot.Phase is not ConnectorConnectionPhase.Connected)
            {
                return false;
            }

            _snapshot = _snapshot with { LastPongAt = receivedAt };
            return true;
        }
    }

    public bool MarkFailed(ConnectorConnectionLease lease, string error)
    {
        lock (_gate)
        {
            if (!IsCurrent(lease) || !_snapshot.ShouldMaintainConnection)
            {
                return false;
            }

            _snapshot = _snapshot with
            {
                Phase = ConnectorConnectionPhase.WaitingToReconnect,
                ConsecutiveFailures = Math.Min(_snapshot.ConsecutiveFailures + 1, 6),
                ConnectedAt = null,
                LastPongAt = null,
                LastError = error,
            };
            return true;
        }
    }

    public ConnectorConnectionLease BeginReconnect()
    {
        lock (_gate)
        {
            if (_snapshot.Phase is not ConnectorConnectionPhase.WaitingToReconnect)
            {
                throw new InvalidOperationException("The connector is not waiting to reconnect.");
            }

            var generation = _snapshot.Generation + 1;
            _snapshot = _snapshot with
            {
                Phase = ConnectorConnectionPhase.Connecting,
                Generation = generation,
            };
            return new(generation);
        }
    }

    public void Suspend()
    {
        lock (_gate)
        {
            if (_snapshot.Phase is ConnectorConnectionPhase.Unconfigured)
            {
                return;
            }

            _snapshot = _snapshot with
            {
                Phase = ConnectorConnectionPhase.Suspended,
                Generation = _snapshot.Generation + 1,
                ConnectedAt = null,
                LastPongAt = null,
            };
        }
    }

    public ConnectorConnectionLease Resume()
    {
        lock (_gate)
        {
            if (_snapshot.Phase is not ConnectorConnectionPhase.Suspended)
            {
                throw new InvalidOperationException("The connector is not suspended.");
            }

            var generation = _snapshot.Generation + 1;
            _snapshot = _snapshot with
            {
                Phase = ConnectorConnectionPhase.Connecting,
                Generation = generation,
                LastError = null,
            };
            return new(generation);
        }
    }

    public void Stop()
    {
        lock (_gate)
        {
            if (_snapshot.Phase is ConnectorConnectionPhase.Unconfigured)
            {
                return;
            }

            _snapshot = new(
                ConnectorConnectionPhase.Stopped,
                _snapshot.Generation + 1,
                0,
                null,
                null,
                null);
        }
    }

    public bool Stop(ConnectorConnectionLease lease)
    {
        lock (_gate)
        {
            if (!IsCurrent(lease))
            {
                return false;
            }

            _snapshot = new(
                ConnectorConnectionPhase.Stopped,
                _snapshot.Generation + 1,
                0,
                null,
                null,
                null);
            return true;
        }
    }

    private bool IsCurrent(ConnectorConnectionLease lease) =>
        lease.Generation == _snapshot.Generation;
}
