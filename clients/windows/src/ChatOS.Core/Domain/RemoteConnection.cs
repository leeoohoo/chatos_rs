namespace ChatOS.Core.Domain;

public enum RemoteAuthenticationType
{
    PrivateKey,
    PrivateKeyCertificate,
    Password,
}

public enum RemoteHostKeyPolicy
{
    Strict,
    AcceptNew,
}

public sealed record RemoteConnection(
    string Id,
    string Name,
    string Host,
    int Port,
    string Username,
    RemoteAuthenticationType AuthenticationType,
    bool HasPassword,
    bool HasPrivateKeyPath,
    bool HasCertificatePath,
    string? DefaultRemotePath,
    RemoteHostKeyPolicy HostKeyPolicy,
    string LocalConnectorDeviceId,
    string LocalConnectorWorkspaceId,
    bool JumpEnabled,
    string? JumpConnectionId,
    string? JumpHost,
    int? JumpPort,
    string? JumpUsername,
    bool HasJumpPrivateKeyPath,
    bool HasJumpCertificatePath,
    bool HasJumpPassword,
    DateTimeOffset? LastActiveAt);

public sealed record RemoteConnectionDraft(
    string? Name,
    string Host,
    int Port,
    string Username,
    RemoteAuthenticationType AuthenticationType,
    string? Password,
    string? PrivateKeyPath,
    string? CertificatePath,
    string? DefaultRemotePath,
    RemoteHostKeyPolicy HostKeyPolicy,
    string LocalConnectorDeviceId,
    string LocalConnectorWorkspaceId,
    bool JumpEnabled,
    string? JumpConnectionId,
    string? JumpHost,
    int? JumpPort,
    string? JumpUsername,
    string? JumpPrivateKeyPath,
    string? JumpCertificatePath,
    string? JumpPassword,
    string? LocalCredentialReferenceId = null);

public sealed record RemoteConnectionTestResult(bool Success, string? Message);

public sealed class RemoteVerificationRequiredException : Exception
{
    public RemoteVerificationRequiredException(string prompt) : base(prompt)
    {
        Prompt = prompt;
    }

    public string Prompt { get; }
}
