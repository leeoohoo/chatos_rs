using ChatOS.Connector.Security;
using Windows.Security.Credentials;

namespace ChatOS.Connector.Persistence;

public sealed class WindowsCredentialConnectorSecretStore : IConnectorSecretStore
{
    private const string ResourceName = "ChatOS.Windows.LocalConnectorSecrets";
    private readonly PasswordVault _vault = new();

    public ValueTask<string?> GetAsync(
        string key,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        try
        {
            var credential = _vault.Retrieve(ResourceName, key);
            credential.RetrievePassword();
            return ValueTask.FromResult<string?>(credential.Password);
        }
        catch
        {
            return ValueTask.FromResult<string?>(null);
        }
    }

    public ValueTask SetAsync(
        string key,
        string value,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (string.IsNullOrWhiteSpace(key))
        {
            throw new ArgumentException("Secret key is required.", nameof(key));
        }

        Delete(key);
        _vault.Add(new PasswordCredential(ResourceName, key, value));
        return ValueTask.CompletedTask;
    }

    public ValueTask DeleteAsync(
        string key,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        Delete(key);
        return ValueTask.CompletedTask;
    }

    private void Delete(string key)
    {
        try
        {
            _vault.Remove(_vault.Retrieve(ResourceName, key));
        }
        catch
        {
            // PasswordVault reports missing credentials as an exception.
        }
    }
}
