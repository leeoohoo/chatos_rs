using System.Text;
using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed partial class AgentTeamToolExecutor
{
    private async Task<AgentToolExecutionResult> ReadAttachmentAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var attachmentReference = RequiredString(arguments, "attachment_ref");
        var authority = references.Attachment(attachmentReference);
        var attachmentId = authority?.AttachmentId ?? (references.AllowsLegacyIds
            ? attachmentReference : throw AgentTeamValidation.Invalid("attachment_ref"));
        var attachmentRoomId = authority?.RoomId ?? room.Id;
        var attachment = authority is null
            ? await store.GetMessageAttachmentAsync(profile.OwnerUserId, attachmentRoomId,
                attachmentId, cancellationToken).ConfigureAwait(false)
            : await store.GetMessageAttachmentForMessageAsync(profile.OwnerUserId,
                attachmentRoomId, authority.MessageId, attachmentId, cancellationToken)
                .ConfigureAwait(false);
        if (attachment is null)
            throw new AgentTeamException(AgentTeamError.NotFound,
                "Message attachment was not found in an authorized conversation.");
        var mimeType = attachment.MimeType.Split(';', 2)[0].Trim().ToLowerInvariant();
        if (!mimeType.StartsWith("text/", StringComparison.Ordinal) && mimeType is not
            ("application/json" or "application/xml" or "application/javascript" or
             "application/yaml" or "application/toml" or "application/sql"))
        {
            throw new AgentTeamException(AgentTeamError.InvalidField,
                "Message attachment is not a supported text format.");
        }
        string text;
        try
        {
            text = new UTF8Encoding(false, true).GetString(attachment.Data);
        }
        catch (DecoderFallbackException exception)
        {
            throw new AgentTeamException(AgentTeamError.InvalidField,
                "Message attachment is not UTF-8 text.", exception);
        }
        var offset = OptionalInt(arguments, "offset") ?? 0;
        var limit = OptionalInt(arguments, "limit") ?? 12_000;
        if (offset < 0 || offset > text.Length || limit is < 1 or > 12_000)
            throw AgentTeamValidation.Invalid("attachment range");
        if (offset < text.Length && char.IsLowSurrogate(text[offset]))
            throw AgentTeamValidation.Invalid("attachment offset");
        var length = Math.Min(limit, text.Length - offset);
        if (length > 0 && offset + length < text.Length &&
            char.IsHighSurrogate(text[offset + length - 1]) &&
            char.IsLowSurrogate(text[offset + length]))
        {
            if (length == 1) length++;
            else length--;
        }
        var nextOffset = offset + length < text.Length ? offset + length : (int?)null;
        return new AgentToolExecutionResult(Json(new
        {
            attachment_ref = authority is null ? attachmentReference :
                references.AttachmentReference(
                    attachmentRoomId, authority.MessageId, attachment.Id),
            attachment.Name,
            attachment.MimeType,
            attachment.ByteCount,
            content = text.Substring(offset, length),
            offset,
            next_offset = nextOffset,
            truncated = nextOffset is not null,
        }));
    }
}
