using ChatOS.Api.Http;
namespace ChatOS.Connector.Persistence;

public sealed class WindowsCredentialTokenStore : IAuthTokenStore
{
    private readonly WindowsCredentialStore _store = new(
        "ChatOS.Windows.ApiAccessToken",
        "current-user");

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
            throw new ArgumentException("Access token cannot be empty.", nameof(token));
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
