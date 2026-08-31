namespace ChatOS.Connector.Runtime;

public interface IConnectorAccessTokenStore
{
    ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default);

    ValueTask SetAccessTokenAsync(string token, CancellationToken cancellationToken = default);

    ValueTask ClearAsync(CancellationToken cancellationToken = default);
}
