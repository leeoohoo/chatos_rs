namespace ChatOS.Connector.Connection;

public interface IConnectorSocket : IAsyncDisposable
{
    Task SendTextAsync(string payload, CancellationToken cancellationToken);

    Task<string?> ReceiveTextAsync(CancellationToken cancellationToken);

    Task CloseAsync(CancellationToken cancellationToken);
}

public interface IConnectorSocketFactory
{
    Task<IConnectorSocket> ConnectAsync(
        ConnectorSocketRequest request,
        CancellationToken cancellationToken);
}
