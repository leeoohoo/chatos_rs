using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed partial class AgentTeamToolExecutor
{
    private async Task<AgentToolExecutionResult> ReadUnreadAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var messages = await store.ListUnreadMessagesAsync(profile.OwnerUserId, room.Id,
            profile.Id, OptionalInt(arguments, "limit") ?? 50, cancellationToken)
            .ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            conversation_ref = references.ConversationReference(room.Id),
            messages = messages.Select(value => MessageResponse(value, references)),
            marked_read = false,
        }));
    }

    private async Task<AgentToolExecutionResult> ReadAllUnreadAsync(
        AgentProfile profile,
        AgentRunReferenceVault references,
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
                conversation_ref = references.ConversationReference(value.Room.Id),
                name = value.Room.Draft.Name,
                kind = value.Room.Kind.ToString(),
                messages = value.Messages.Select(message => MessageResponse(message, references)),
            }),
            message_count = conversations.Sum(value => value.Messages.Count),
            marked_read = true,
        }));
    }

    private async Task<AgentToolExecutionResult> MarkReadAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var throughMessageReference = RequiredString(arguments, "through_message_ref");
        var authority = references.Message(throughMessageReference);
        var throughMessageId = authority?.MessageId ?? (references.AllowsLegacyIds
            ? throughMessageReference : throw AgentTeamValidation.Invalid("through_message_ref"));
        if (authority is not null && authority.RoomId != room.Id)
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Message reference does not belong to the current conversation.");
        await store.MarkReadAsync(profile.OwnerUserId, room.Id, profile.Id, throughMessageId,
            cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            conversation_ref = references.ConversationReference(room.Id),
            through_message_ref = references.MessageReference(room.Id, throughMessageId),
            marked_read = true,
        }));
    }

    private static object MessageResponse(
        AgentMessage message,
        AgentRunReferenceVault references) => new
    {
        message_ref = references.MessageReference(message.RoomId, message.Id),
        sender = message.SenderKind.ToString(),
        sender_agent_ref = message.SenderAgentId is null ? null :
            references.AgentReference(message.SenderAgentId),
        message.Content,
        reply_to_message_ref = message.ReplyToMessageId is null ? null :
            references.MessageReference(message.RoomId, message.ReplyToMessageId),
        attachments = message.Attachments.Select(value => new
        {
            attachment_ref = references.AttachmentReference(message.RoomId, value.Id),
            value.Name,
            value.MimeType,
            value.Kind,
            value.ByteCount,
        }),
        message.CreatedAtUnixMs,
    };
}
