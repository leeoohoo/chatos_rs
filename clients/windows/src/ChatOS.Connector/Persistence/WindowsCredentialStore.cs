using Windows.Security.Credentials;

namespace ChatOS.Connector.Persistence;

internal sealed class WindowsCredentialStore(string resourceName, string userName)
{
    private readonly PasswordVault _vault = new();

    public string? Get()
    {
        try
        {
            var credential = _vault.Retrieve(resourceName, userName);
            credential.RetrievePassword();
            return credential.Password;
        }
        catch
        {
            return null;
        }
    }

    public void Set(string token)
    {
        Remove();
        _vault.Add(new PasswordCredential(resourceName, userName, token));
    }

    public void Remove()
    {
        try
        {
            foreach (var credential in _vault.FindAllByResource(resourceName))
            {
                _vault.Remove(credential);
            }
        }
        catch
        {
            // PasswordVault throws when a resource has no stored credentials.
        }
    }
}
