using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalRemoteConnection(
    string ConnectionId,
    string OwnerUserId,
    string Name,
    string Host,
    uint Port,
    string Username,
    string AuthenticationType,
    bool HasPassword,
    bool HasPrivateKeyPath,
    bool HasCertificatePath,
    string? DefaultRemotePath,
    string HostKeyPolicy,
    string LocalConnectorDeviceId,
    string LocalConnectorWorkspaceId,
    bool JumpEnabled,
    string? JumpConnectionId,
    string? JumpHost,
    uint? JumpPort,
    string? JumpUsername,
    bool HasJumpPrivateKeyPath,
    bool HasJumpCertificatePath,
    bool HasJumpPassword,
    long? LastActiveAtUnixMs,
    ulong Version,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs);

internal sealed record LocalRemoteConnectionSpec(
    string? Name,
    string Host,
    uint Port,
    string Username,
    string AuthenticationType,
    string? DefaultRemotePath,
    string HostKeyPolicy,
    string LocalConnectorDeviceId,
    string LocalConnectorWorkspaceId,
    bool JumpEnabled,
    string? JumpConnectionId,
    string? JumpHost,
    uint? JumpPort,
    string? JumpUsername)
{
    public static LocalRemoteConnectionSpec From(RemoteConnectionDraft draft) => new(
        draft.Name,
        draft.Host,
        checked((uint)draft.Port),
        draft.Username,
        draft.AuthenticationType switch
        {
            RemoteAuthenticationType.Password => "password",
            RemoteAuthenticationType.PrivateKeyCertificate => "private_key_cert",
            _ => "private_key",
        },
        draft.DefaultRemotePath,
        draft.HostKeyPolicy == RemoteHostKeyPolicy.AcceptNew ? "accept_new" : "strict",
        draft.LocalConnectorDeviceId,
        draft.LocalConnectorWorkspaceId,
        draft.JumpEnabled,
        draft.JumpConnectionId,
        draft.JumpHost,
        draft.JumpPort is { } jumpPort ? checked((uint)jumpPort) : null,
        draft.JumpUsername);
}

internal sealed record LocalRemoteOwnerCommand(string Type, string OwnerUserId);
internal sealed record LocalRemoteIdentityCommand(
    string Type,
    string OwnerUserId,
    string ConnectionId);
internal sealed record LocalRemoteCreateCommand(
    string Type,
    string OwnerUserId,
    LocalRemoteConnectionSpec Spec);
internal sealed record LocalRemoteUpdateCommand(
    string Type,
    string OwnerUserId,
    string ConnectionId,
    ulong ExpectedVersion,
    LocalRemoteConnectionSpec Spec);
internal sealed record LocalRemoteDeleteCommand(
    string Type,
    string OwnerUserId,
    string ConnectionId,
    ulong ExpectedVersion);

internal sealed record LocalRemoteConnectionsResult(
    string Type,
    IReadOnlyList<WindowsLocalRemoteConnection> Connections);
internal sealed record LocalRemoteConnectionResult(
    string Type,
    WindowsLocalRemoteConnection? Connection);
internal sealed record LocalRemoteConnectionDeletedResult(string Type, string ConnectionId);

public sealed class WindowsLocalAgentRemoteConnectionClient(ILocalAgentHostClient host)
{
    public async Task<IReadOnlyList<WindowsLocalRemoteConnection>> ListAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<LocalRemoteOwnerCommand, LocalRemoteConnectionsResult>(
            new("list_remote_connections", ownerUserId),
            cancellationToken).ConfigureAwait(false);
        RequireType(response.Type, "remote_connections");
        return response.Connections;
    }

    public async Task<WindowsLocalRemoteConnection?> GetAsync(
        string ownerUserId,
        string connectionId,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<LocalRemoteIdentityCommand, LocalRemoteConnectionResult>(
            new("get_remote_connection", ownerUserId, connectionId),
            cancellationToken).ConfigureAwait(false);
        RequireType(response.Type, "remote_connection");
        return response.Connection;
    }

    public async Task<WindowsLocalRemoteConnection> CreateAsync(
        string ownerUserId,
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<LocalRemoteCreateCommand, LocalRemoteConnectionResult>(
            new("create_remote_connection", ownerUserId, LocalRemoteConnectionSpec.From(draft)),
            cancellationToken).ConfigureAwait(false);
        return RequireConnection(response);
    }

    public async Task<WindowsLocalRemoteConnection> UpdateAsync(
        string ownerUserId,
        string connectionId,
        ulong expectedVersion,
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<LocalRemoteUpdateCommand, LocalRemoteConnectionResult>(
            new(
                "update_remote_connection",
                ownerUserId,
                connectionId,
                expectedVersion,
                LocalRemoteConnectionSpec.From(draft)),
            cancellationToken).ConfigureAwait(false);
        return RequireConnection(response);
    }

    public async Task DeleteAsync(
        string ownerUserId,
        string connectionId,
        ulong expectedVersion,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<LocalRemoteDeleteCommand, LocalRemoteConnectionDeletedResult>(
            new("delete_remote_connection", ownerUserId, connectionId, expectedVersion),
            cancellationToken).ConfigureAwait(false);
        RequireType(response.Type, "remote_connection_deleted");
        if (!string.Equals(response.ConnectionId, connectionId, StringComparison.Ordinal))
        {
            throw new InvalidDataException("Local Agent Host deleted a different remote connection.");
        }
    }

    private static WindowsLocalRemoteConnection RequireConnection(LocalRemoteConnectionResult result)
    {
        RequireType(result.Type, "remote_connection");
        return result.Connection ?? throw new InvalidDataException(
            "Local Agent Host did not return a remote connection.");
    }

    private static void RequireType(string actual, string expected)
    {
        if (!string.Equals(actual, expected, StringComparison.Ordinal))
        {
            throw new InvalidDataException($"Expected {expected}, received {actual}.");
        }
    }
}
