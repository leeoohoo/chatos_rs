using System.Net;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;

namespace ChatOS.Api.Pet;

/// Temporary user-activity control boundary. Main Chat never calls this
/// remote endpoint; it remains scoped only to the Pet activity slice until
/// that slice is projected entirely from the Local Agent Host.
public sealed class PetConversationControlService(ChatOSApiClient client) : IPetConversationControl
{
    public async Task StopTurnAsync(
        string conversationId,
        string? turnId,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(conversationId);
        var response = await client.PostAsync<StopChatResponseDto>(
            "agent/chat/stop",
            new StopChatRequestDto(conversationId, Trimmed(turnId)),
            cancellationToken).ConfigureAwait(false);
        if (!response.Success)
        {
            throw new ChatOSApiException(
                Trimmed(response.Message) ?? "The active AI turn could not be stopped.",
                HttpStatusCode.Conflict);
        }
    }

    private static string? Trimmed(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private sealed record StopChatRequestDto(
        [property: JsonPropertyName("conversation_id")] string ConversationId,
        [property: JsonPropertyName("turn_id")] string? TurnId);

    private sealed record StopChatResponseDto(
        [property: JsonPropertyName("success")] bool Success,
        [property: JsonPropertyName("message")] string? Message);
}
