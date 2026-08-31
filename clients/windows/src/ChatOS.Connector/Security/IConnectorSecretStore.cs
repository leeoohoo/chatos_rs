namespace ChatOS.Connector.Security;

public interface IConnectorSecretStore
{
    ValueTask<string?> GetAsync(string key, CancellationToken cancellationToken = default);

    ValueTask SetAsync(string key, string value, CancellationToken cancellationToken = default);

    ValueTask DeleteAsync(string key, CancellationToken cancellationToken = default);
}
