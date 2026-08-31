namespace ChatOS.Api.Http;

public interface IAuthTokenStore
{
    ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default);

    ValueTask SetAccessTokenAsync(string token, CancellationToken cancellationToken = default);

    ValueTask ClearAsync(CancellationToken cancellationToken = default);
}
