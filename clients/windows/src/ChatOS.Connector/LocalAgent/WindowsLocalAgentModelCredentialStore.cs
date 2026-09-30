using Windows.Security.Credentials;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentModelCredentialStore
{
    private const string Resource = "ChatOS.LocalAgent.Model";
    private readonly PasswordVault _vault = new();

    public string? Load(string ownerUserId, string modelConfigRef)
    {
        try
        {
            var credential = _vault.Retrieve(Resource, Account(ownerUserId, modelConfigRef));
            credential.RetrievePassword();
            return string.IsNullOrEmpty(credential.Password) ? null : credential.Password;
        }
        catch (Exception exception) when (
            exception.HResult == unchecked((int)0x80070490))
        {
            return null;
        }
    }

    public void Save(string credential, string ownerUserId, string modelConfigRef)
    {
        if (string.IsNullOrEmpty(credential) ||
            System.Text.Encoding.UTF8.GetByteCount(credential) > 64 * 1024 ||
            credential.Contains('\0'))
        {
            throw new ArgumentException("Local Agent model credential is invalid.", nameof(credential));
        }
        Delete(ownerUserId, modelConfigRef);
        _vault.Add(new PasswordCredential(
            Resource,
            Account(ownerUserId, modelConfigRef),
            credential));
    }

    public void Delete(string ownerUserId, string modelConfigRef)
    {
        try
        {
            var credential = _vault.Retrieve(Resource, Account(ownerUserId, modelConfigRef));
            _vault.Remove(credential);
        }
        catch (Exception exception) when (
            exception.HResult == unchecked((int)0x80070490))
        {
        }
    }

    public static string EnvironmentVariable(string modelConfigRef)
    {
        var normalized = new string(modelConfigRef
            .ToUpperInvariant()
            .Select(character => char.IsAsciiLetterUpper(character) || char.IsAsciiDigit(character)
                ? character
                : '_')
            .Take(96)
            .ToArray());
        return $"CHATOS_LOCAL_AGENT_MODEL_{normalized}";
    }

    private static string Account(string ownerUserId, string modelConfigRef) =>
        $"v1:{ownerUserId}:{modelConfigRef}";
}
