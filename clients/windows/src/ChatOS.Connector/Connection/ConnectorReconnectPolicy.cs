namespace ChatOS.Connector.Connection;

public sealed class ConnectorReconnectPolicy
{
    private static readonly TimeSpan[] DefaultDelays =
    [
        TimeSpan.FromSeconds(1),
        TimeSpan.FromSeconds(2),
        TimeSpan.FromSeconds(4),
        TimeSpan.FromSeconds(8),
        TimeSpan.FromSeconds(16),
        TimeSpan.FromSeconds(30),
    ];

    private readonly IReadOnlyList<TimeSpan> _delays;

    public ConnectorReconnectPolicy()
        : this(DefaultDelays)
    {
    }

    internal ConnectorReconnectPolicy(IReadOnlyList<TimeSpan> delays)
    {
        if (delays.Count == 0 || delays.Any(delay => delay < TimeSpan.Zero))
        {
            throw new ArgumentException("Reconnect delays must contain non-negative values.", nameof(delays));
        }

        _delays = delays.ToArray();
    }

    public TimeSpan DelayAfterFailure(int consecutiveFailures)
    {
        if (consecutiveFailures <= 0)
        {
            return TimeSpan.Zero;
        }

        return _delays[Math.Min(consecutiveFailures - 1, _delays.Count - 1)];
    }
}
