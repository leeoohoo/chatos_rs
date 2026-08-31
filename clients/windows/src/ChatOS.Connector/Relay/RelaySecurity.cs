namespace ChatOS.Connector.Relay;

public sealed record RemoteControlTrust(
    bool RequireSignedMessages,
    int SignatureMaxSkewSeconds,
    IReadOnlyDictionary<string, string> TrustedRelayPublicKeys);

public sealed record RelaySecurityContext(
    string OwnerUserId,
    string DeviceId,
    RemoteControlTrust Trust);

public interface IRelaySecurityContextProvider
{
    Task<RelaySecurityContext> GetAsync(CancellationToken cancellationToken);
}
