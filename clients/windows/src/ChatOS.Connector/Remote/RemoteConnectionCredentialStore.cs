using System.Text.Json;
using ChatOS.Connector.Security;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Remote;

public sealed record RemoteConnectionCredentials(
    string? Password,
    string? PrivateKeyPath,
    string? CertificatePath,
    string? JumpPrivateKeyPath,
    string? JumpCertificatePath,
    string? JumpPassword)
{
    public bool IsEmpty => new[]
    {
        Password,
        PrivateKeyPath,
        CertificatePath,
        JumpPrivateKeyPath,
        JumpCertificatePath,
        JumpPassword,
    }.All(string.IsNullOrWhiteSpace);

    public static RemoteConnectionCredentials From(RemoteConnectionDraft draft) => new(
        draft.AuthenticationType == RemoteAuthenticationType.Password ? Clean(draft.Password) : null,
        draft.AuthenticationType == RemoteAuthenticationType.Password ? null : Clean(draft.PrivateKeyPath),
        draft.AuthenticationType == RemoteAuthenticationType.PrivateKeyCertificate
            ? Clean(draft.CertificatePath)
            : null,
        Clean(draft.JumpPrivateKeyPath),
        Clean(draft.JumpCertificatePath),
        Clean(draft.JumpPassword));

    public RemoteConnectionDraft Apply(RemoteConnectionDraft draft) => draft with
    {
        Password = Clean(draft.Password) ?? Password,
        PrivateKeyPath = Clean(draft.PrivateKeyPath) ?? PrivateKeyPath,
        CertificatePath = Clean(draft.CertificatePath) ?? CertificatePath,
        JumpPrivateKeyPath = Clean(draft.JumpPrivateKeyPath) ?? JumpPrivateKeyPath,
        JumpCertificatePath = Clean(draft.JumpCertificatePath) ?? JumpCertificatePath,
        JumpPassword = Clean(draft.JumpPassword) ?? JumpPassword,
    };

    private static string? Clean(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}

public sealed class RemoteConnectionCredentialStore
{
    private readonly IConnectorSecretStore _secrets;

    public RemoteConnectionCredentialStore(IConnectorSecretStore secrets)
    {
        _secrets = secrets;
    }

    public async Task<RemoteConnectionCredentials?> LoadAsync(
        string connectionId,
        CancellationToken cancellationToken = default)
    {
        var value = await _secrets.GetAsync(Key(connectionId), cancellationToken).ConfigureAwait(false);
        return string.IsNullOrWhiteSpace(value)
            ? null
            : JsonSerializer.Deserialize<RemoteConnectionCredentials>(value)
                ?? throw new InvalidDataException("远程连接凭据格式无效。");
    }

    public async Task SaveAsync(
        string connectionId,
        RemoteConnectionCredentials credentials,
        CancellationToken cancellationToken = default)
    {
        if (credentials.IsEmpty)
        {
            await DeleteAsync(connectionId, cancellationToken).ConfigureAwait(false);
            return;
        }

        await _secrets.SetAsync(
            Key(connectionId),
            JsonSerializer.Serialize(credentials),
            cancellationToken).ConfigureAwait(false);
    }

    public Task DeleteAsync(string connectionId, CancellationToken cancellationToken = default) =>
        _secrets.DeleteAsync(Key(connectionId), cancellationToken).AsTask();

    private static string Key(string connectionId) =>
        $"remote-connection-credentials-v1:{connectionId}";
}
