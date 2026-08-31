using System.Net.WebSockets;
using System.Text;

namespace ChatOS.Connector.Connection;

public sealed class ClientWebSocketConnectorSocketFactory : IConnectorSocketFactory
{
    public async Task<IConnectorSocket> ConnectAsync(
        ConnectorSocketRequest request,
        CancellationToken cancellationToken)
    {
        var socket = new ClientWebSocket();
        foreach (var header in request.Headers)
        {
            socket.Options.SetRequestHeader(header.Key, header.Value);
        }

        try
        {
            await socket.ConnectAsync(request.Uri, cancellationToken).ConfigureAwait(false);
            return new ClientWebSocketConnectorSocket(socket);
        }
        catch
        {
            socket.Dispose();
            throw;
        }
    }
}

internal sealed class ClientWebSocketConnectorSocket : IConnectorSocket
{
    private const int MaximumMessageBytes = 4 * 1024 * 1024;
    private readonly ClientWebSocket _socket;
    private readonly SemaphoreSlim _sendGate = new(1, 1);

    public ClientWebSocketConnectorSocket(ClientWebSocket socket)
    {
        _socket = socket;
    }

    public async Task SendTextAsync(string payload, CancellationToken cancellationToken)
    {
        var bytes = Encoding.UTF8.GetBytes(payload);
        await _sendGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _socket.SendAsync(
                bytes,
                WebSocketMessageType.Text,
                endOfMessage: true,
                cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            _sendGate.Release();
        }
    }

    public async Task<string?> ReceiveTextAsync(CancellationToken cancellationToken)
    {
        using var stream = new MemoryStream();
        var buffer = new byte[16 * 1024];
        while (true)
        {
            var result = await _socket.ReceiveAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (result.MessageType is WebSocketMessageType.Close)
            {
                return null;
            }

            if (result.MessageType is not WebSocketMessageType.Text)
            {
                if (result.EndOfMessage)
                {
                    return string.Empty;
                }

                continue;
            }

            stream.Write(buffer, 0, result.Count);
            if (stream.Length > MaximumMessageBytes)
            {
                throw new InvalidDataException("Connector WebSocket message exceeded 4 MB.");
            }

            if (result.EndOfMessage)
            {
                return Encoding.UTF8.GetString(stream.GetBuffer(), 0, checked((int)stream.Length));
            }
        }
    }

    public async Task CloseAsync(CancellationToken cancellationToken)
    {
        if (_socket.State is WebSocketState.Open or WebSocketState.CloseReceived)
        {
            await _socket.CloseOutputAsync(
                WebSocketCloseStatus.NormalClosure,
                "ChatOS connector stopping",
                cancellationToken).ConfigureAwait(false);
        }
    }

    public ValueTask DisposeAsync()
    {
        _socket.Dispose();
        _sendGate.Dispose();
        return ValueTask.CompletedTask;
    }
}
