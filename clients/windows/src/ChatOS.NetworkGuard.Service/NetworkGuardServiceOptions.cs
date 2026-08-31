namespace ChatOS.NetworkGuard.Service;

public sealed class NetworkGuardServiceOptions
{
    public Dictionary<string, string> TrustedPolicyPublicKeys { get; init; } =
        new(StringComparer.Ordinal);

    public TimeSpan LeaseDuration { get; init; } = TimeSpan.FromMinutes(2);

    public int HttpBrokerPort { get; init; } = 49180;

    public int HttpsBrokerPort { get; init; } = 49443;

    public TimeSpan HandshakeTimeout { get; init; } = TimeSpan.FromSeconds(5);

    public TimeSpan ConnectTimeout { get; init; } = TimeSpan.FromSeconds(10);
}
