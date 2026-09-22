using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    public async Task<IReadOnlyList<AgentMessage>> ListMessagesAsync(
        string ownerUserId,
        string roomId,
        int limit = 200,
        bool includeAttachmentPayloads = false,
        CancellationToken cancellationToken = default)
    {
        if (limit is < 1 or > 1_000)
        {
            throw AgentTeamValidation.Invalid(nameof(limit));
        }

        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var rows = new List<MessageRow>();
        using (var command = Command(connection, null, """
            SELECT id, sender_kind, sender_agent_id, content, reply_to_message_id,
                root_message_id, hop_count, created_at_unix_ms
            FROM agent_messages
            WHERE owner_user_id = @p0 AND room_id = @p1
            ORDER BY created_at_unix_ms DESC, id DESC
            LIMIT @p2
            """, ownerUserId, roomId, limit))
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                rows.Add(new MessageRow(
                    reader.GetString(0),
                    ParseEnum<AgentMessageSenderKind>(reader.GetString(1)),
                    reader.IsDBNull(2) ? null : reader.GetString(2),
                    reader.GetString(3),
                    reader.IsDBNull(4) ? null : reader.GetString(4),
                    reader.GetString(5),
                    reader.GetInt32(6),
                    reader.GetInt64(7)));
            }
        }

        rows.Reverse();
        var mentions = rows.ToDictionary(value => value.Id,
            _ => new List<string>(), StringComparer.Ordinal);
        using (var mentionCommand = Command(connection, null, """
            WITH recent AS (
                SELECT id FROM agent_messages
                WHERE owner_user_id = @p0 AND room_id = @p1
                ORDER BY created_at_unix_ms DESC, id DESC LIMIT @p2
            )
            SELECT m.message_id, m.agent_id FROM agent_message_mentions m
            JOIN recent r ON r.id = m.message_id
            WHERE m.owner_user_id = @p0 ORDER BY m.message_id, m.agent_id
            """, ownerUserId, roomId, limit))
        await using (var reader = await mentionCommand.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                mentions[reader.GetString(0)].Add(reader.GetString(1));
        }

        var attachments = rows.ToDictionary(value => value.Id,
            _ => new List<AgentMessageAttachment>(), StringComparer.Ordinal);
        var payloadColumn = includeAttachmentPayloads
            ? "COALESCE(p.payload, a.payload)"
            : "zeroblob(0)";
        var payloadJoin = includeAttachmentPayloads
            ? """
              LEFT JOIN agent_message_attachment_payloads p
                ON p.owner_user_id = a.owner_user_id
               AND p.message_id = a.message_id AND p.id = a.id
              """
            : string.Empty;
        using (var attachmentCommand = Command(connection, null, $"""
            WITH recent AS (
                SELECT id FROM agent_messages
                WHERE owner_user_id = @p0 AND room_id = @p1
                ORDER BY created_at_unix_ms DESC, id DESC LIMIT @p2
            )
            SELECT a.message_id, a.id, a.name, a.mime_type, a.kind, a.byte_count,
                {payloadColumn}
            FROM agent_message_attachments a JOIN recent r ON r.id = a.message_id
            {payloadJoin}
            WHERE a.owner_user_id = @p0 ORDER BY a.message_id, a.id
            """, ownerUserId, roomId, limit))
        await using (var reader = await attachmentCommand.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                attachments[reader.GetString(0)].Add(new AgentMessageAttachment(
                    reader.GetString(1), reader.GetString(2), reader.GetString(3),
                    ParseEnum<AgentMessageAttachmentKind>(reader.GetString(4)),
                    reader.GetInt64(5), (byte[])reader[6]));
            }
        }

        return rows.Select(row => new AgentMessage(row.Id, ownerUserId, roomId, row.SenderKind,
            row.SenderAgentId, row.Content, mentions[row.Id], attachments[row.Id],
            row.ReplyToMessageId, row.RootMessageId, row.HopCount, row.CreatedAtUnixMs)).ToArray();
    }

    public async Task<AgentMessage?> GetMessageAsync(
        string ownerUserId,
        string roomId,
        string messageId,
        CancellationToken cancellationToken = default,
        bool includeAttachmentPayloads = false)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null, """
            SELECT id, sender_kind, sender_agent_id, content, reply_to_message_id,
                root_message_id, hop_count, created_at_unix_ms
            FROM agent_messages
            WHERE owner_user_id = @p0 AND room_id = @p1 AND id = @p2
            """, ownerUserId, roomId, messageId);
        MessageRow? row;
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            row = await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
                ? new MessageRow(
                    reader.GetString(0),
                    ParseEnum<AgentMessageSenderKind>(reader.GetString(1)),
                    reader.IsDBNull(2) ? null : reader.GetString(2),
                    reader.GetString(3),
                    reader.IsDBNull(4) ? null : reader.GetString(4),
                    reader.GetString(5),
                    reader.GetInt32(6),
                    reader.GetInt64(7))
                : null;
        }

        return row is null
            ? null
            : await MaterializeMessageAsync(
                connection, null, ownerUserId, roomId, row, includeAttachmentPayloads,
                cancellationToken).ConfigureAwait(false);
    }

    public async Task<AgentMessageAttachment?> GetMessageAttachmentAsync(
        string ownerUserId,
        string roomId,
        string attachmentId,
        CancellationToken cancellationToken = default)
        => await GetMessageAttachmentCoreAsync(ownerUserId, roomId, null, attachmentId,
            cancellationToken).ConfigureAwait(false);

    public async Task<AgentMessageAttachment?> GetMessageAttachmentForMessageAsync(
        string ownerUserId,
        string roomId,
        string messageId,
        string attachmentId,
        CancellationToken cancellationToken = default)
        => await GetMessageAttachmentCoreAsync(ownerUserId, roomId, messageId, attachmentId,
            cancellationToken).ConfigureAwait(false);

    private async Task<AgentMessageAttachment?> GetMessageAttachmentCoreAsync(
        string ownerUserId,
        string roomId,
        string? messageId,
        string attachmentId,
        CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null, """
            SELECT a.id, a.name, a.mime_type, a.kind, a.byte_count,
                COALESCE(p.payload, a.payload)
            FROM agent_message_attachments a
            JOIN agent_messages m ON m.owner_user_id = a.owner_user_id AND m.id = a.message_id
            LEFT JOIN agent_message_attachment_payloads p
              ON p.owner_user_id = a.owner_user_id
             AND p.message_id = a.message_id AND p.id = a.id
            WHERE a.owner_user_id = @p0 AND m.room_id = @p1 AND a.id = @p2
              AND (@p3 IS NULL OR a.message_id = @p3)
            """, ownerUserId, roomId, attachmentId, DbValue(messageId));
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
            ? new AgentMessageAttachment(reader.GetString(0), reader.GetString(1), reader.GetString(2),
                ParseEnum<AgentMessageAttachmentKind>(reader.GetString(3)), reader.GetInt64(4),
                (byte[])reader[5])
            : null;
    }

    public async Task<AgentPostResult> PostMessageAsync(
        string ownerUserId,
        string roomId,
        AgentMessageDraft draft,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(roomId, nameof(roomId));
        draft.Validate();
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await RequireRoomAsync(connection, transaction, ownerUserId, roomId, requireActive: true,
            cancellationToken).ConfigureAwait(false);
        if (draft.SenderKind == AgentMessageSenderKind.Agent)
        {
            await RequireActiveMemberAsync(connection, transaction, ownerUserId, roomId,
                draft.SenderAgentId!, cancellationToken).ConfigureAwait(false);
        }

        if (draft.ReplyToMessageId is not null)
        {
            await RequireMessageAsync(connection, transaction, ownerUserId, roomId,
                draft.ReplyToMessageId, cancellationToken).ConfigureAwait(false);
        }

        var now = Now();
        var messageId = NewId();
        var rootMessageId = draft.RootMessageId ?? messageId;
        if (draft.RootMessageId is not null)
        {
            await RequireMessageAsync(connection, transaction, ownerUserId, roomId,
                draft.RootMessageId, cancellationToken).ConfigureAwait(false);
        }

        using (var command = Command(connection, transaction, """
            INSERT INTO agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_agent_id, content,
                reply_to_message_id, root_message_id, hop_count, created_at_unix_ms)
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, @p8, @p9)
            """, ownerUserId, messageId, roomId, draft.SenderKind.ToString(),
            DbValue(draft.SenderAgentId), draft.Content, DbValue(draft.ReplyToMessageId),
            rootMessageId, draft.HopCount, now))
        {
            await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        foreach (var agentId in draft.Mentions)
        {
            await RequireActiveMemberAsync(connection, transaction, ownerUserId, roomId,
                agentId, cancellationToken).ConfigureAwait(false);
            using var mention = Command(connection, transaction, """
                INSERT INTO agent_message_mentions(owner_user_id, message_id, agent_id)
                VALUES (@p0, @p1, @p2)
                """, ownerUserId, messageId, agentId);
            await mention.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        foreach (var attachment in draft.AttachmentItems)
        {
            using var insert = Command(connection, transaction, """
                INSERT INTO agent_message_attachments (
                    owner_user_id, message_id, id, name, mime_type, kind, byte_count, payload)
                VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7)
                """, ownerUserId, messageId, attachment.Id, attachment.Name,
                attachment.MimeType, attachment.Kind.ToString(), attachment.ByteCount,
                Array.Empty<byte>());
            await insert.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
            using var insertPayload = Command(connection, transaction, """
                INSERT INTO agent_message_attachment_payloads (
                    owner_user_id, message_id, id, payload)
                VALUES (@p0, @p1, @p2, @p3)
                """, ownerUserId, messageId, attachment.Id, attachment.Data);
            await insertPayload.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        var targets = await ResolveTargetsAsync(
            connection, transaction, ownerUserId, roomId, draft, cancellationToken).ConfigureAwait(false);
        var existingRuns = await CountRootDeliveriesAsync(
            connection, transaction, ownerUserId, rootMessageId, cancellationToken).ConfigureAwait(false);
        var available = Math.Max(0, 12 - existingRuns);
        var deliveries = new List<AgentDelivery>();
        string? stopReason = null;
        if (draft.HopCount >= 4 && targets.Count > 0)
        {
            stopReason = "maximum_hop_count";
            targets = [];
        }
        else if (targets.Count > available)
        {
            stopReason = "maximum_agent_runs";
            targets = targets.Take(available).ToArray();
        }

        foreach (var (agentId, trigger) in targets)
        {
            var delivery = await InsertDeliveryAsync(
                connection, transaction, ownerUserId, roomId, messageId, rootMessageId,
                agentId, trigger, draft.HopCount, $"message:{messageId}:agent:{agentId}", now,
                cancellationToken).ConfigureAwait(false);
            if (delivery is not null)
            {
                deliveries.Add(delivery);
            }
        }

        using (var touch = Command(connection, transaction, """
            UPDATE agent_rooms SET updated_at_unix_ms = @p0
            WHERE owner_user_id = @p1 AND id = @p2
            """, now, ownerUserId, roomId))
        {
            await touch.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        var message = new AgentMessage(messageId, ownerUserId, roomId, draft.SenderKind,
            draft.SenderAgentId, draft.Content, draft.Mentions, draft.AttachmentItems,
            draft.ReplyToMessageId, rootMessageId, draft.HopCount, now);
        return new AgentPostResult(message, deliveries, stopReason);
    }

    private static async Task<IReadOnlyList<(string AgentId, AgentDeliveryTrigger Trigger)>> ResolveTargetsAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        AgentMessageDraft draft,
        CancellationToken cancellationToken)
    {
        if (draft.Mentions.Count > 0)
        {
            return draft.Mentions
                .Where(value => !string.Equals(value, draft.SenderAgentId, StringComparison.Ordinal))
                .Distinct(StringComparer.Ordinal)
                .Select(value => (value, draft.SenderKind == AgentMessageSenderKind.Agent
                    ? AgentDeliveryTrigger.AgentMention
                    : AgentDeliveryTrigger.Mention))
                .ToArray();
        }

        if (draft.SenderKind == AgentMessageSenderKind.Agent)
        {
            return [];
        }

        using var command = Command(connection, transaction, """
            SELECT default_agent_id FROM agent_rooms
            WHERE owner_user_id = @p0 AND id = @p1
            """, ownerUserId, roomId);
        var defaultAgentId = await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) as string;
        return defaultAgentId is null
            ? []
            : [(defaultAgentId, AgentDeliveryTrigger.DefaultAgent)];
    }

    private static async Task<int> CountRootDeliveriesAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string rootMessageId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            SELECT COUNT(*) FROM agent_deliveries
            WHERE owner_user_id = @p0 AND root_message_id = @p1
            """, ownerUserId, rootMessageId);
        return Convert.ToInt32(await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false));
    }

    private static async Task RequireMessageAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        string messageId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            SELECT 1 FROM agent_messages
            WHERE owner_user_id = @p0 AND room_id = @p1 AND id = @p2
            """, ownerUserId, roomId, messageId);
        if (await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) is null)
        {
            throw NotFound("Agent message");
        }
    }

    private static async Task<AgentMessage> MaterializeMessageAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        string roomId,
        MessageRow row,
        bool includeAttachmentPayloads,
        CancellationToken cancellationToken)
    {
        var mentions = new List<string>();
        using (var mentionCommand = Command(connection, transaction, """
            SELECT agent_id FROM agent_message_mentions
            WHERE owner_user_id = @p0 AND message_id = @p1 ORDER BY agent_id
            """, ownerUserId, row.Id))
        await using (var reader = await mentionCommand.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                mentions.Add(reader.GetString(0));
            }
        }

        var attachments = new List<AgentMessageAttachment>();
        var payloadColumn = includeAttachmentPayloads
            ? "COALESCE(p.payload, a.payload)"
            : "zeroblob(0)";
        var payloadJoin = includeAttachmentPayloads
            ? """
              LEFT JOIN agent_message_attachment_payloads p
                ON p.owner_user_id = a.owner_user_id
               AND p.message_id = a.message_id AND p.id = a.id
              """
            : string.Empty;
        using (var attachmentCommand = Command(connection, transaction, $"""
            SELECT a.id, a.name, a.mime_type, a.kind, a.byte_count, {payloadColumn}
            FROM agent_message_attachments a
            {payloadJoin}
            WHERE a.owner_user_id = @p0 AND a.message_id = @p1 ORDER BY a.id
            """, ownerUserId, row.Id))
        await using (var reader = await attachmentCommand.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                attachments.Add(new AgentMessageAttachment(
                    reader.GetString(0), reader.GetString(1), reader.GetString(2),
                    ParseEnum<AgentMessageAttachmentKind>(reader.GetString(3)),
                    reader.GetInt64(4), (byte[])reader[5]));
            }
        }

        var message = new AgentMessage(row.Id, ownerUserId, roomId, row.SenderKind,
            row.SenderAgentId, row.Content, mentions, attachments, row.ReplyToMessageId,
            row.RootMessageId, row.HopCount, row.CreatedAtUnixMs);
        message.Validate();
        return message;
    }

    private sealed record MessageRow(
        string Id,
        AgentMessageSenderKind SenderKind,
        string? SenderAgentId,
        string Content,
        string? ReplyToMessageId,
        string RootMessageId,
        int HopCount,
        long CreatedAtUnixMs);
}
