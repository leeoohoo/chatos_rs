using System.Buffers;
using System.Net.WebSockets;
using System.Runtime.CompilerServices;
using System.Text;
using System.Text.Json;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using Microsoft.Extensions.Options;

namespace ChatOS.Api.Realtime;

public sealed class ChatOSRealtimeClient : IRealtimeClient
{
    private const int MaximumMessageBytes = 2 * 1024 * 1024;
    private readonly WebSocketTicketService _ticketService;
    private readonly ChatOSApiOptions _options;

    public ChatOSRealtimeClient(
        WebSocketTicketService ticketService,
        IOptions<ChatOSApiOptions> options)
    {
        _ticketService = ticketService;
        _options = options.Value;
    }

    public async IAsyncEnumerable<ConversationRealtimeSignal> StreamConversationAsync(
        string conversationId,
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(conversationId);
        var reconnectAttempt = 0;
        while (!cancellationToken.IsCancellationRequested)
        {
            var failed = false;
            await using (var enumerator = ConnectAsync(
                ConversationSubscription(conversationId),
                cancellationToken).GetAsyncEnumerator(cancellationToken))
            {
                while (true)
                {
                    var next = await MoveNextSafelyAsync(enumerator, cancellationToken).ConfigureAwait(false);
                    if (next.Cancelled)
                    {
                        yield break;
                    }

                    if (!next.HasValue)
                    {
                        failed = next.Failed;
                        break;
                    }

                    var signal = ConversationRealtimeEventDecoder.Decode(next.Value!, conversationId);
                    if (signal is not null)
                    {
                        yield return signal;
                    }
                }
            }

            reconnectAttempt = failed ? reconnectAttempt + 1 : 0;
            await DelayBeforeReconnectAsync(reconnectAttempt, cancellationToken).ConfigureAwait(false);
        }
    }

    public async IAsyncEnumerable<PetActivityEvent> StreamPetActivitiesAsync(
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        var reconnectAttempt = 0;
        while (!cancellationToken.IsCancellationRequested)
        {
            yield return new PetActivityEvent.Reconcile();
            var failed = false;
            await using (var enumerator = ConnectAsync(
                UserSubscription(),
                cancellationToken).GetAsyncEnumerator(cancellationToken))
            {
                while (true)
                {
                    var next = await MoveNextSafelyAsync(enumerator, cancellationToken).ConfigureAwait(false);
                    if (next.Cancelled)
                    {
                        yield break;
                    }

                    if (!next.HasValue)
                    {
                        failed = next.Failed;
                        break;
                    }

                    var activityEvent = PetRealtimeEventDecoder.Decode(next.Value!);
                    if (activityEvent is not null)
                    {
                        yield return activityEvent;
                    }
                }
            }

            reconnectAttempt = failed ? reconnectAttempt + 1 : 0;
            await DelayBeforeReconnectAsync(reconnectAttempt, cancellationToken).ConfigureAwait(false);
        }
    }

    internal Uri BuildWebSocketUri(string ticket)
    {
        var baseUri = new Uri(_options.BaseUrl.EndsWith("/", StringComparison.Ordinal)
            ? _options.BaseUrl
            : $"{_options.BaseUrl}/", UriKind.Absolute);
        var builder = new UriBuilder(new Uri(baseUri, "realtime/ws"))
        {
            Scheme = baseUri.Scheme switch
            {
                "https" => "wss",
                "http" => "ws",
                _ => throw new InvalidOperationException("ChatOS API base URL must use HTTP or HTTPS."),
            },
            Query = $"ws_ticket={Uri.EscapeDataString(ticket)}",
        };
        return builder.Uri;
    }

    internal static string ConversationSubscription(string conversationId) =>
        JsonSerializer.Serialize(new
        {
            type = "subscribe",
            topics = new[] { new { scope = "conversation", id = conversationId } },
        });

    internal static string UserSubscription() =>
        JsonSerializer.Serialize(new
        {
            type = "subscribe",
            topics = new[] { new { scope = "user" } },
        });

    private async IAsyncEnumerable<string> ConnectAsync(
        string subscription,
        [EnumeratorCancellation] CancellationToken cancellationToken)
    {
        var ticket = await _ticketService.IssueAsync(cancellationToken).ConfigureAwait(false);
        using var socket = new ClientWebSocket();
        socket.Options.KeepAliveInterval = TimeSpan.FromSeconds(20);
        await socket.ConnectAsync(BuildWebSocketUri(ticket), cancellationToken).ConfigureAwait(false);

        var subscriptionBytes = Encoding.UTF8.GetBytes(subscription);
        await socket.SendAsync(
            subscriptionBytes,
            WebSocketMessageType.Text,
            true,
            cancellationToken).ConfigureAwait(false);

        while (socket.State == WebSocketState.Open && !cancellationToken.IsCancellationRequested)
        {
            var message = await ReceiveTextMessageAsync(socket, cancellationToken).ConfigureAwait(false);
            if (message is null)
            {
                yield break;
            }

            yield return message;
        }
    }

    private static async Task<string?> ReceiveTextMessageAsync(
        ClientWebSocket socket,
        CancellationToken cancellationToken)
    {
        var buffer = ArrayPool<byte>.Shared.Rent(16 * 1024);
        try
        {
            using var stream = new MemoryStream();
            while (true)
            {
                var result = await socket.ReceiveAsync(buffer, cancellationToken).ConfigureAwait(false);
                if (result.MessageType == WebSocketMessageType.Close)
                {
                    if (socket.State == WebSocketState.CloseReceived)
                    {
                        await socket.CloseOutputAsync(
                            WebSocketCloseStatus.NormalClosure,
                            "acknowledged",
                            cancellationToken).ConfigureAwait(false);
                    }

                    return null;
                }

                if (result.MessageType != WebSocketMessageType.Text)
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
                    throw new WebSocketException("Realtime message exceeded the client safety limit.");
                }

                if (result.EndOfMessage)
                {
                    return Encoding.UTF8.GetString(stream.GetBuffer(), 0, checked((int)stream.Length));
                }
            }
        }
        finally
        {
            ArrayPool<byte>.Shared.Return(buffer);
        }
    }

    private static Task DelayBeforeReconnectAsync(
        int attempt,
        CancellationToken cancellationToken)
    {
        var exponent = Math.Clamp(attempt - 1, 0, 5);
        var seconds = Math.Min(30, 1 << exponent);
        return Task.Delay(TimeSpan.FromSeconds(seconds), cancellationToken);
    }

    private static async Task<MoveNextResult> MoveNextSafelyAsync(
        IAsyncEnumerator<string> enumerator,
        CancellationToken cancellationToken)
    {
        try
        {
            return await enumerator.MoveNextAsync().ConfigureAwait(false)
                ? new MoveNextResult(true, enumerator.Current, false, false)
                : new MoveNextResult(false, null, false, false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            return new MoveNextResult(false, null, false, true);
        }
        catch (WebSocketException)
        {
            return new MoveNextResult(false, null, true, false);
        }
        catch (ChatOSApiException)
        {
            return new MoveNextResult(false, null, true, false);
        }
    }

    private readonly record struct MoveNextResult(
        bool HasValue,
        string? Value,
        bool Failed,
        bool Cancelled);
}
