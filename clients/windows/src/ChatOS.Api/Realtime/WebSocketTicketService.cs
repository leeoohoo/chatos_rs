using System.Text.Json.Serialization;
using ChatOS.Api.Http;

namespace ChatOS.Api.Realtime;

public sealed class WebSocketTicketService
{
    private readonly ChatOSApiClient _client;

    public WebSocketTicketService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<string> IssueAsync(CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<WebSocketTicketDto>(
            "auth/ws-ticket",
            cancellationToken: cancellationToken).ConfigureAwait(false);
        var ticket = response.Ticket.Trim();
        if (ticket.Length == 0)
        {
            throw new ChatOSApiException("The ChatOS gateway did not issue a WebSocket ticket.");
        }

        return ticket;
    }
}

internal sealed record WebSocketTicketDto(
    [property: JsonPropertyName("ticket")] string Ticket);
