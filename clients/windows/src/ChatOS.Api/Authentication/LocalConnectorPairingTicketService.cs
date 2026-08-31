using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;

namespace ChatOS.Api.Authentication;

public sealed class LocalConnectorPairingTicketService : ILocalConnectorPairingTicketService
{
    private readonly ChatOSApiClient _client;

    public LocalConnectorPairingTicketService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<string> IssueAsync(CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<PairingTicketResponse>(
            "auth/local-connector-ticket",
            cancellationToken: cancellationToken).ConfigureAwait(false);
        return string.IsNullOrWhiteSpace(response.Ticket)
            ? throw new ChatOSApiException("配对响应缺少 ticket。")
            : response.Ticket;
    }
}

internal sealed record PairingTicketResponse(
    [property: JsonPropertyName("ticket")] string Ticket);
