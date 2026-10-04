using ChatOS.Connector.Runtime;
namespace ChatOS.Connector.Persistence;

public sealed class WindowsCredentialConnectorTokenStore : IConnectorAccessTokenStore
{
    private readonly WindowsCredentialStore _store = new(
        "ChatOS.Windows.GatewayAccessToken",
        "current-device");

    public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return ValueTask.FromResult(_store.Get());
    }

    public ValueTask SetAccessTokenAsync(string token, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (string.IsNullOrWhiteSpace(token))
        {
            throw new ArgumentException("Connector access token cannot be empty.", nameof(token));
        }

        _store.Set(token);
        return ValueTask.CompletedTask;
    }

    public ValueTask ClearAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        _store.Remove();
        return ValueTask.CompletedTask;
    }
}
