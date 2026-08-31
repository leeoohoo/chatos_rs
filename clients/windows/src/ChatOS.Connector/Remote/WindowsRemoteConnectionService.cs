using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Remote;

public interface IRemoteConnectionTester
{
    Task<RemoteConnectionTestResult> TestAsync(
        RemoteConnectionDraft draft,
        string? verificationCode,
        CancellationToken cancellationToken = default);
}

public interface IRemoteConnectionRuntime
{
    Task<RemoteConnectionDraft> ResolveDraftAsync(string id, CancellationToken cancellationToken = default);
}

public sealed class WindowsRemoteConnectionService : IRemoteConnectionService, IRemoteConnectionRuntime
{
    private readonly IRemoteConnectionCloudService _cloud;
    private readonly RemoteConnectionCredentialStore _credentials;
    private readonly IRemoteConnectionTester _tester;

    public WindowsRemoteConnectionService(
        IRemoteConnectionCloudService cloud,
        RemoteConnectionCredentialStore credentials,
        IRemoteConnectionTester tester)
    {
        _cloud = cloud;
        _credentials = credentials;
        _tester = tester;
    }

    public async Task<IReadOnlyList<RemoteConnection>> ListAsync(
        CancellationToken cancellationToken = default)
    {
        var values = await _cloud.ListAsync(cancellationToken).ConfigureAwait(false);
        var result = new List<RemoteConnection>(values.Count);
        foreach (var value in values)
        {
            result.Add(await DecorateAsync(value, cancellationToken).ConfigureAwait(false));
        }
        return result;
    }

    public async Task<RemoteConnection> CreateAsync(
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default)
    {
        var resolved = await ResolveAsync(draft, cancellationToken).ConfigureAwait(false);
        var created = await _cloud.CreateAsync(Sanitize(resolved), cancellationToken).ConfigureAwait(false);
        try
        {
            await _credentials.SaveAsync(
                created.Id,
                RemoteConnectionCredentials.From(resolved),
                cancellationToken).ConfigureAwait(false);
        }
        catch
        {
            try { await _cloud.DeleteAsync(created.Id, CancellationToken.None).ConfigureAwait(false); }
            catch { }
            throw;
        }
        return await DecorateAsync(created, cancellationToken).ConfigureAwait(false);
    }

    public async Task<RemoteConnection> UpdateAsync(
        string id,
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default)
    {
        var resolved = await ResolveAsync(
            draft with { LocalCredentialReferenceId = id },
            cancellationToken).ConfigureAwait(false);
        var updated = await _cloud.UpdateAsync(id, Sanitize(resolved), cancellationToken).ConfigureAwait(false);
        await _credentials.SaveAsync(
            id,
            RemoteConnectionCredentials.From(resolved),
            cancellationToken).ConfigureAwait(false);
        return await DecorateAsync(updated, cancellationToken).ConfigureAwait(false);
    }

    public async Task DeleteAsync(string id, CancellationToken cancellationToken = default)
    {
        await _cloud.DeleteAsync(id, cancellationToken).ConfigureAwait(false);
        await _credentials.DeleteAsync(id, cancellationToken).ConfigureAwait(false);
    }

    public async Task<RemoteConnectionTestResult> TestDraftAsync(
        RemoteConnectionDraft draft,
        string? verificationCode,
        CancellationToken cancellationToken = default) =>
        await _tester.TestAsync(
            await ResolveAsync(draft, cancellationToken).ConfigureAwait(false),
            verificationCode,
            cancellationToken).ConfigureAwait(false);

    public async Task<RemoteConnectionTestResult> TestSavedAsync(
        string id,
        string? verificationCode,
        CancellationToken cancellationToken = default)
    {
        return await _tester.TestAsync(
            await ResolveDraftAsync(id, cancellationToken).ConfigureAwait(false),
            verificationCode,
            cancellationToken).ConfigureAwait(false);
    }

    public async Task<RemoteConnectionDraft> ResolveDraftAsync(
        string id,
        CancellationToken cancellationToken = default)
    {
        var connections = await _cloud.ListAsync(cancellationToken).ConfigureAwait(false);
        var connection = connections.FirstOrDefault(value => value.Id == id)
            ?? throw new InvalidOperationException("远程连接不存在。");
        return await ResolveAsync(
            Draft(connection, connections) with { LocalCredentialReferenceId = id },
            cancellationToken).ConfigureAwait(false);
    }

    private async Task<RemoteConnectionDraft> ResolveAsync(
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken)
    {
        var resolved = draft;
        if (!string.IsNullOrWhiteSpace(draft.LocalCredentialReferenceId) &&
            await _credentials.LoadAsync(draft.LocalCredentialReferenceId, cancellationToken)
                .ConfigureAwait(false) is { } stored)
        {
            resolved = stored.Apply(resolved);
        }
        if (!string.IsNullOrWhiteSpace(draft.JumpConnectionId) &&
            await _credentials.LoadAsync(draft.JumpConnectionId, cancellationToken)
                .ConfigureAwait(false) is { } jump)
        {
            resolved = resolved with
            {
                JumpPrivateKeyPath = Clean(resolved.JumpPrivateKeyPath) ?? jump.PrivateKeyPath,
                JumpCertificatePath = Clean(resolved.JumpCertificatePath) ?? jump.CertificatePath,
                JumpPassword = Clean(resolved.JumpPassword) ?? jump.Password,
            };
        }
        return resolved;
    }

    private async Task<RemoteConnection> DecorateAsync(
        RemoteConnection connection,
        CancellationToken cancellationToken)
    {
        var stored = await _credentials.LoadAsync(connection.Id, cancellationToken).ConfigureAwait(false);
        return stored is null ? connection : connection with
        {
            HasPassword = !string.IsNullOrWhiteSpace(stored.Password),
            HasPrivateKeyPath = !string.IsNullOrWhiteSpace(stored.PrivateKeyPath),
            HasCertificatePath = !string.IsNullOrWhiteSpace(stored.CertificatePath),
            HasJumpPrivateKeyPath = !string.IsNullOrWhiteSpace(stored.JumpPrivateKeyPath),
            HasJumpCertificatePath = !string.IsNullOrWhiteSpace(stored.JumpCertificatePath),
            HasJumpPassword = !string.IsNullOrWhiteSpace(stored.JumpPassword),
        };
    }

    private static RemoteConnectionDraft Sanitize(RemoteConnectionDraft draft) => draft with
    {
        Password = null,
        PrivateKeyPath = null,
        CertificatePath = null,
        JumpPrivateKeyPath = null,
        JumpCertificatePath = null,
        JumpPassword = null,
        LocalCredentialReferenceId = null,
    };

    private static RemoteConnectionDraft Draft(
        RemoteConnection value,
        IReadOnlyList<RemoteConnection> all)
    {
        var jump = string.IsNullOrWhiteSpace(value.JumpConnectionId)
            ? null
            : all.FirstOrDefault(item => item.Id == value.JumpConnectionId);
        return new RemoteConnectionDraft(
            value.Name,
            value.Host,
            value.Port,
            value.Username,
            value.AuthenticationType,
            null,
            null,
            null,
            value.DefaultRemotePath,
            value.HostKeyPolicy,
            value.LocalConnectorDeviceId,
            value.LocalConnectorWorkspaceId,
            value.JumpEnabled,
            value.JumpConnectionId,
            jump?.Host ?? value.JumpHost,
            jump?.Port ?? value.JumpPort,
            jump?.Username ?? value.JumpUsername,
            null,
            null,
            null,
            value.Id);
    }

    private static string? Clean(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}
