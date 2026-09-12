using ChatOS.Connector.Runtime;
using Windows.Security.Credentials;

namespace ChatOS.Connector.Persistence;

public sealed class WindowsCredentialConnectorTokenStore : IConnectorAccessTokenStore
{
    private const string ResourceName = "ChatOS.Windows.GatewayAccessToken";
    private const string UserName = "current-device";
    private readonly PasswordVault _vault = new();

    public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        try
        {
            var credential = _vault.Retrieve(ResourceName, UserName);
            credential.RetrievePassword();
            return ValueTask.FromResult<string?>(credential.Password);
        }
        catch
        {
            return ValueTask.FromResult<string?>(null);
        }
    }

    public ValueTask SetAccessTokenAsync(string token, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (string.IsNullOrWhiteSpace(token))
        {
            throw new ArgumentException("Connector access token cannot be empty.", nameof(token));
        }

        RemoveExisting();
        _vault.Add(new PasswordCredential(ResourceName, UserName, token));
        return ValueTask.CompletedTask;
    }

    public ValueTask ClearAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        RemoveExisting();
        return ValueTask.CompletedTask;
    }

    private void RemoveExisting()
    {
        try
        {
            _vault.Remove(_vault.Retrieve(ResourceName, UserName));
        }
        catch
        {
            // PasswordVault reports an absent exact credential as an exception.
        }
    }
}
