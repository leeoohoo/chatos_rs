using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed partial class AgentTeamToolExecutor
{
    private async Task<AgentToolExecutionResult> ReadUnreadAsync(
        AgentProfile profile,
        AgentRoom room,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var messages = await store.ListUnreadMessagesAsync(profile.OwnerUserId, room.Id,
            profile.Id, OptionalInt(arguments, "limit") ?? 50, cancellationToken)
            .ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            room_id = room.Id,
            messages = messages.Select(MessageResponse),
            marked_read = false,
        }));
    }

    private async Task<AgentToolExecutionResult> ReadAllUnreadAsync(
        AgentProfile profile,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var conversations = await store.ReadAllUnreadMessagesAndMarkReadAsync(
            profile.OwnerUserId, profile.Id, OptionalInt(arguments, "limit") ?? 200,
            cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            conversations = conversations.Select(value => new
            {
                room_id = value.Room.Id,
                name = value.Room.Draft.Name,
                kind = value.Room.Kind.ToString(),
                messages = value.Messages.Select(MessageResponse),
            }),
            message_count = conversations.Sum(value => value.Messages.Count),
            marked_read = true,
        }));
    }

    private async Task<AgentToolExecutionResult> MarkReadAsync(
        AgentProfile profile,
        AgentRoom room,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var throughMessageId = RequiredString(arguments, "through_message_id");
        await store.MarkReadAsync(profile.OwnerUserId, room.Id, profile.Id, throughMessageId,
            cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            room_id = room.Id,
            through_message_id = throughMessageId,
            marked_read = true,
        }));
    }

    private static object MessageResponse(AgentMessage message) => new
    {
        message_id = message.Id,
        sender = message.SenderKind.ToString(),
        sender_agent_id = message.SenderAgentId,
        message.Content,
        reply_to_message_id = message.ReplyToMessageId,
        attachments = message.Attachments.Select(value => new
        {
            attachment_id = value.Id,
            value.Name,
            value.MimeType,
            value.Kind,
            value.ByteCount,
        }),
        message.CreatedAtUnixMs,
    };
}
