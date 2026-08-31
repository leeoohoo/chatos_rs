namespace ChatOS.Connector.Connection;

public enum ConnectorConnectionPhase
{
    Unconfigured,
    Stopped,
    Connecting,
    Connected,
    WaitingToReconnect,
    Suspended,
}

public sealed record ConnectorConnectionSnapshot(
    ConnectorConnectionPhase Phase,
    long Generation,
    int ConsecutiveFailures,
    DateTimeOffset? ConnectedAt,
    DateTimeOffset? LastPongAt,
    string? LastError)
{
    public bool IsConfigured => Phase is not ConnectorConnectionPhase.Unconfigured;

    public bool ShouldMaintainConnection =>
        Phase is ConnectorConnectionPhase.Connecting
            or ConnectorConnectionPhase.Connected
            or ConnectorConnectionPhase.WaitingToReconnect;
}

public readonly record struct ConnectorConnectionLease(long Generation);
