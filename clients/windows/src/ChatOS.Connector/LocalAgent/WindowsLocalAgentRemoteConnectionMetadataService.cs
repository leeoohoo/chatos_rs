using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentRemoteConnectionMetadataService(
    WindowsLocalAgentRemoteConnectionClient client) : IRemoteConnectionMetadataService
{
    private readonly object _gate = new();
    private readonly Dictionary<string, ulong> _versions = new(StringComparer.Ordinal);
    private string? _ownerUserId;

    public void Configure(string ownerUserId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(ownerUserId);
        lock (_gate)
        {
            _ownerUserId = ownerUserId;
            _versions.Clear();
        }
    }

    public void Reset()
    {
        lock (_gate)
        {
            _ownerUserId = null;
            _versions.Clear();
        }
    }

    public async Task<IReadOnlyList<RemoteConnection>> ListAsync(
        CancellationToken cancellationToken = default)
    {
        var records = await client.ListAsync(Owner(), cancellationToken).ConfigureAwait(false);
        lock (_gate)
        {
            foreach (var record in records) _versions[record.ConnectionId] = record.Version;
        }
        return records.Select(ToDomain).ToArray();
    }

    public async Task<RemoteConnection> CreateAsync(
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default)
    {
        var record = await client.CreateAsync(Owner(), draft, cancellationToken).ConfigureAwait(false);
        Cache(record);
        return ToDomain(record);
    }

    public async Task<RemoteConnection> UpdateAsync(
        string id,
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default)
    {
        var owner = Owner();
        var version = await VersionAsync(owner, id, cancellationToken).ConfigureAwait(false);
        var record = await client.UpdateAsync(owner, id, version, draft, cancellationToken)
            .ConfigureAwait(false);
        Cache(record);
        return ToDomain(record);
    }

    public async Task DeleteAsync(string id, CancellationToken cancellationToken = default)
    {
        var owner = Owner();
        var version = await VersionAsync(owner, id, cancellationToken).ConfigureAwait(false);
        await client.DeleteAsync(owner, id, version, cancellationToken).ConfigureAwait(false);
        lock (_gate) _versions.Remove(id);
    }

    private async Task<ulong> VersionAsync(
        string ownerUserId,
        string connectionId,
        CancellationToken cancellationToken)
    {
        lock (_gate)
        {
            if (_versions.TryGetValue(connectionId, out var version)) return version;
        }
        var record = await client.GetAsync(ownerUserId, connectionId, cancellationToken)
            .ConfigureAwait(false) ?? throw new InvalidOperationException("远程连接不存在。");
        Cache(record);
        return record.Version;
    }

    private void Cache(WindowsLocalRemoteConnection record)
    {
        lock (_gate) _versions[record.ConnectionId] = record.Version;
    }

    private string Owner()
    {
        lock (_gate)
        {
            return _ownerUserId ?? throw new InvalidOperationException(
                "Local remote connections are not configured for an account.");
        }
    }

    private static RemoteConnection ToDomain(WindowsLocalRemoteConnection value) => new(
        value.ConnectionId,
        value.Name,
        value.Host,
        checked((int)value.Port),
        value.Username,
        value.AuthenticationType switch
        {
            "password" => RemoteAuthenticationType.Password,
            "private_key_cert" => RemoteAuthenticationType.PrivateKeyCertificate,
            _ => RemoteAuthenticationType.PrivateKey,
        },
        value.HasPassword,
        value.HasPrivateKeyPath,
        value.HasCertificatePath,
        value.DefaultRemotePath,
        value.HostKeyPolicy == "accept_new" ? RemoteHostKeyPolicy.AcceptNew : RemoteHostKeyPolicy.Strict,
        value.LocalConnectorDeviceId,
        value.LocalConnectorWorkspaceId,
        value.JumpEnabled,
        value.JumpConnectionId,
        value.JumpHost,
        value.JumpPort is { } jumpPort ? checked((int)jumpPort) : null,
        value.JumpUsername,
        value.HasJumpPrivateKeyPath,
        value.HasJumpCertificatePath,
        value.HasJumpPassword,
        value.LastActiveAtUnixMs is { } timestamp
            ? DateTimeOffset.FromUnixTimeMilliseconds(timestamp)
            : null);
}
