namespace ChatOS.Connector.Connection;

public sealed class ConnectorHeartbeatMonitor
{
    private readonly object _gate = new();
    private DateTimeOffset? _lastPongAt;
    private int _missedAcknowledgements;

    public int MissedAcknowledgements
    {
        get
        {
            lock (_gate)
            {
                return _missedAcknowledgements;
            }
        }
    }

    public void Reset(DateTimeOffset connectedAt)
    {
        lock (_gate)
        {
            _lastPongAt = connectedAt;
            _missedAcknowledgements = 0;
        }
    }

    public void RecordPong(DateTimeOffset receivedAt)
    {
        lock (_gate)
        {
            if (_lastPongAt is null || receivedAt > _lastPongAt)
            {
                _lastPongAt = receivedAt;
            }
        }
    }

    public bool CompleteHeartbeat(DateTimeOffset sentAt)
    {
        lock (_gate)
        {
            if (_lastPongAt >= sentAt)
            {
                _missedAcknowledgements = 0;
            }
            else
            {
                _missedAcknowledgements++;
            }

            return _missedAcknowledgements >= 3;
        }
    }
}
