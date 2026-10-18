using System.Net;
using System.Net.Http;
using System.Net.WebSockets;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Connection;

public sealed class ClientWebSocketConnectorSocketFactory : IConnectorSocketFactory
{
    public async Task<IConnectorSocket> ConnectAsync(
        ConnectorSocketRequest request,
        CancellationToken cancellationToken)
    {
        var socket = new ClientWebSocket();
        var handler = new ConnectorHandshakeResponseHandler(new SocketsHttpHandler());
        var invoker = new HttpMessageInvoker(handler);
        foreach (var header in request.Headers)
        {
            socket.Options.SetRequestHeader(header.Key, header.Value);
        }

        try
        {
            await socket.ConnectAsync(request.Uri, invoker, cancellationToken).ConfigureAwait(false);
            return new ClientWebSocketConnectorSocket(socket, invoker);
        }
        catch (WebSocketException exception)
        {
            socket.Dispose();
            invoker.Dispose();
            if (exception.InnerException is ConnectorHandshakeException handshake)
            {
                throw handshake;
            }

            throw;
        }
        catch
        {
            socket.Dispose();
            invoker.Dispose();
            throw;
        }
    }
}

internal sealed class ConnectorHandshakeResponseHandler(HttpMessageHandler innerHandler)
    : DelegatingHandler(innerHandler)
{
    private const int MaximumErrorBytes = 64 * 1024;

    protected override async Task<HttpResponseMessage> SendAsync(
        HttpRequestMessage request,
        CancellationToken cancellationToken)
    {
        var response = await base.SendAsync(request, cancellationToken).ConfigureAwait(false);
        if (response.StatusCode is HttpStatusCode.SwitchingProtocols)
        {
            return response;
        }

        using (response)
        {
            string? payload = null;
            try
            {
                await response.Content.LoadIntoBufferAsync(MaximumErrorBytes)
                    .ConfigureAwait(false);
                payload = await response.Content.ReadAsStringAsync(cancellationToken)
                    .ConfigureAwait(false);
            }
            catch when (!cancellationToken.IsCancellationRequested)
            {
                // A response body is helpful but not required to surface the HTTP status.
            }

            throw ConnectorHandshakeException.FromResponse(response.StatusCode, payload);
        }
    }
}

internal sealed class ConnectorHandshakeException : IOException
{
    private ConnectorHandshakeException(HttpStatusCode statusCode, string? code, string message)
        : base(message)
    {
        StatusCode = statusCode;
        Code = code;
    }

    public HttpStatusCode StatusCode { get; }

    public string? Code { get; }

    public static ConnectorHandshakeException FromResponse(
        HttpStatusCode statusCode,
        string? payload)
    {
        var (code, serverMessage) = ReadError(payload);
        var message = code switch
        {
            "connector_already_active" =>
                "同一账号已有另一个 Local Connector 客户端在线。请先在另一台设备上断开 Connector，再重试连接。",
            "connector_session_lease_lost" or "connector_presence_lease_lost" =>
                "Local Connector 会话已被另一台设备替换，正在等待重新连接。",
            _ when !string.IsNullOrWhiteSpace(serverMessage) =>
                $"Local Connector 连接失败（HTTP {(int)statusCode}）：{serverMessage}",
            _ => $"Local Connector 连接失败（HTTP {(int)statusCode} {statusCode}）。",
        };
        return new ConnectorHandshakeException(statusCode, code, message);
    }

    private static (string? Code, string? Message) ReadError(string? payload)
    {
        if (string.IsNullOrWhiteSpace(payload))
        {
            return (null, null);
        }

        try
        {
            using var document = JsonDocument.Parse(payload);
            var root = document.RootElement;
            return (
                ReadString(root, "code"),
                ReadString(root, "message") ?? ReadString(root, "error"));
        }
        catch (JsonException)
        {
            return (null, null);
        }
    }

    private static string? ReadString(JsonElement root, string name) =>
        root.TryGetProperty(name, out var value) &&
        value.ValueKind is JsonValueKind.String &&
        !string.IsNullOrWhiteSpace(value.GetString())
            ? value.GetString()
            : null;
}

internal sealed class ClientWebSocketConnectorSocket : IConnectorSocket
{
    private const int MaximumMessageBytes = 4 * 1024 * 1024;
    private readonly ClientWebSocket _socket;
    private readonly HttpMessageInvoker _httpInvoker;
    private readonly SemaphoreSlim _sendGate = new(1, 1);

    public ClientWebSocketConnectorSocket(
        ClientWebSocket socket,
        HttpMessageInvoker httpInvoker)
    {
        _socket = socket;
        _httpInvoker = httpInvoker;
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
        _httpInvoker.Dispose();
        _sendGate.Dispose();
        return ValueTask.CompletedTask;
    }
}
