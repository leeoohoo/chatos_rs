using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    public async Task<IReadOnlyList<AgentMessage>> ListUnreadMessagesAsync(
        string ownerUserId,
        string roomId,
        string agentId,
        int limit = 50,
        CancellationToken cancellationToken = default)
    {
        if (limit is < 1 or > 100) throw AgentTeamValidation.Invalid(nameof(limit));
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        await RequireAgentAsync(connection, null, ownerUserId, agentId, true,
            cancellationToken).ConfigureAwait(false);
        await RequireRoomAsync(connection, null, ownerUserId, roomId, true,
            cancellationToken).ConfigureAwait(false);
        await RequireActiveMemberAsync(connection, null, ownerUserId, roomId, agentId,
            cancellationToken).ConfigureAwait(false);
        var rows = await ReadUnreadRowsAsync(connection, null, ownerUserId, roomId, agentId,
            limit, cancellationToken).ConfigureAwait(false);
        return await MaterializeRowsAsync(connection, null, ownerUserId, roomId, rows,
            cancellationToken).ConfigureAwait(false);
    }

    public async Task MarkReadAsync(
        string ownerUserId,
        string roomId,
        string readerId,
        string throughMessageId,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(readerId, nameof(readerId));
        AgentTeamValidation.Identifier(throughMessageId, nameof(throughMessageId));
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await RequireActiveMemberAsync(connection, transaction, ownerUserId, roomId, readerId,
            cancellationToken).ConfigureAwait(false);
        var cursor = await ReadMessageCursorAsync(connection, transaction, ownerUserId, roomId,
            throughMessageId, cancellationToken).ConfigureAwait(false)
            ?? throw NotFound("Agent message");
        await AdvanceReadCursorAsync(connection, transaction, ownerUserId, roomId, readerId,
            cursor.Id, cursor.CreatedAtUnixMs, cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task<IReadOnlyList<AgentUnreadConversation>>
        ReadAllUnreadMessagesAndMarkReadAsync(
            string ownerUserId,
            string agentId,
            int limit = 200,
            CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(agentId, nameof(agentId));
        if (limit is < 1 or > 500) throw AgentTeamValidation.Invalid(nameof(limit));
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await RequireAgentAsync(connection, transaction, ownerUserId, agentId, true,
            cancellationToken).ConfigureAwait(false);
        var rows = new List<InboxRow>();
        using (var command = Command(connection, transaction, """
            SELECT msg.id, msg.room_id, msg.sender_kind, msg.sender_agent_id, msg.content,
                   msg.reply_to_message_id, msg.root_message_id, msg.hop_count,
                   msg.created_at_unix_ms
            FROM agent_messages msg
            JOIN agent_rooms room
              ON room.owner_user_id = msg.owner_user_id AND room.id = msg.room_id
            JOIN agent_room_members member
              ON member.owner_user_id = msg.owner_user_id AND member.room_id = msg.room_id
             AND member.agent_id = @p1
            LEFT JOIN agent_read_cursors cursor
              ON cursor.owner_user_id = msg.owner_user_id AND cursor.room_id = msg.room_id
             AND cursor.reader_id = @p1
            WHERE msg.owner_user_id = @p0 AND room.status = 'Active'
              AND member.status = 'Active'
              AND NOT (msg.sender_kind = 'Agent' AND msg.sender_agent_id = @p1)
              AND (cursor.through_message_id IS NULL
                OR msg.created_at_unix_ms > cursor.through_message_created_at_unix_ms
                OR (msg.created_at_unix_ms = cursor.through_message_created_at_unix_ms
                    AND msg.id > cursor.through_message_id))
            ORDER BY msg.created_at_unix_ms, msg.id LIMIT @p2
            """, ownerUserId, agentId, limit))
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                rows.Add(ReadInboxRow(reader));
        }

        if (rows.Count == 0)
        {
            await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
            return [];
        }

        foreach (var last in rows.GroupBy(value => value.RoomId)
                     .Select(group => group.Last()))
            await AdvanceReadCursorAsync(connection, transaction, ownerUserId, last.RoomId,
                agentId, last.Id, last.CreatedAtUnixMs, cancellationToken).ConfigureAwait(false);

        var output = new List<AgentUnreadConversation>();
        foreach (var group in rows.GroupBy(value => value.RoomId))
        {
            var room = await ReadRoomAsync(connection, transaction, ownerUserId, group.Key,
                cancellationToken).ConfigureAwait(false) ?? throw NotFound("Agent room");
            var messages = await MaterializeRowsAsync(connection, transaction, ownerUserId, group.Key,
                group.Select(value => value.Message).ToArray(), cancellationToken)
                .ConfigureAwait(false);
            output.Add(new AgentUnreadConversation(room, messages));
        }
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return output;
    }

    private static async Task<IReadOnlyList<MessageRow>> ReadUnreadRowsAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        string roomId,
        string agentId,
        int limit,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            SELECT msg.id, msg.sender_kind, msg.sender_agent_id, msg.content,
                   msg.reply_to_message_id, msg.root_message_id, msg.hop_count,
                   msg.created_at_unix_ms
            FROM agent_messages msg
            LEFT JOIN agent_read_cursors cursor
              ON cursor.owner_user_id = msg.owner_user_id AND cursor.room_id = msg.room_id
             AND cursor.reader_id = @p2
            WHERE msg.owner_user_id = @p0 AND msg.room_id = @p1
              AND NOT (msg.sender_kind = 'Agent' AND msg.sender_agent_id = @p2)
              AND (cursor.through_message_id IS NULL
                OR msg.created_at_unix_ms > cursor.through_message_created_at_unix_ms
                OR (msg.created_at_unix_ms = cursor.through_message_created_at_unix_ms
                    AND msg.id > cursor.through_message_id))
            ORDER BY msg.created_at_unix_ms, msg.id LIMIT @p3
            """, ownerUserId, roomId, agentId, limit);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false);
        var rows = new List<MessageRow>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            rows.Add(new MessageRow(reader.GetString(0),
                ParseEnum<AgentMessageSenderKind>(reader.GetString(1)),
                reader.IsDBNull(2) ? null : reader.GetString(2), reader.GetString(3),
                reader.IsDBNull(4) ? null : reader.GetString(4), reader.GetString(5),
                reader.GetInt32(6), reader.GetInt64(7)));
        return rows;
    }

    private static async Task<IReadOnlyList<AgentMessage>> MaterializeRowsAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        string roomId,
        IReadOnlyList<MessageRow> rows,
        CancellationToken cancellationToken)
    {
        if (rows.Count == 0) return [];
        var mentions = rows.ToDictionary(value => value.Id, _ => new List<string>(),
            StringComparer.Ordinal);
        var attachments = rows.ToDictionary(value => value.Id,
            _ => new List<AgentMessageAttachment>(), StringComparer.Ordinal);
        var values = new object[rows.Count + 1];
        values[0] = ownerUserId;
        for (var index = 0; index < rows.Count; index++) values[index + 1] = rows[index].Id;
        var placeholders = string.Join(", ", Enumerable.Range(1, rows.Count)
            .Select(index => $"@p{index}"));
        using (var mentionCommand = Command(connection, transaction, $"""
            SELECT message_id, agent_id FROM agent_message_mentions
            WHERE owner_user_id = @p0 AND message_id IN ({placeholders})
            ORDER BY message_id, agent_id
            """, values))
        await using (var reader = await mentionCommand.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                mentions[reader.GetString(0)].Add(reader.GetString(1));
        }
        using (var attachmentCommand = Command(connection, transaction, $"""
            SELECT message_id, id, name, mime_type, kind, byte_count
            FROM agent_message_attachments
            WHERE owner_user_id = @p0 AND message_id IN ({placeholders})
            ORDER BY message_id, id
            """, values))
        await using (var reader = await attachmentCommand.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                attachments[reader.GetString(0)].Add(new AgentMessageAttachment(
                    reader.GetString(1), reader.GetString(2), reader.GetString(3),
                    ParseEnum<AgentMessageAttachmentKind>(reader.GetString(4)),
                    reader.GetInt64(5), []));
        }
        return rows.Select(row => new AgentMessage(row.Id, ownerUserId, roomId,
            row.SenderKind, row.SenderAgentId, row.Content, mentions[row.Id],
            attachments[row.Id], row.ReplyToMessageId, row.RootMessageId, row.HopCount,
            row.CreatedAtUnixMs)).ToArray();
    }

    private static async Task<MessageCursor?> ReadMessageCursorAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        string messageId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            SELECT id, created_at_unix_ms FROM agent_messages
            WHERE owner_user_id = @p0 AND room_id = @p1 AND id = @p2
            """, ownerUserId, roomId, messageId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
            ? new MessageCursor(reader.GetString(0), reader.GetInt64(1)) : null;
    }

    private static async Task AdvanceReadCursorAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        string agentId,
        string messageId,
        long messageCreatedAtUnixMs,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            INSERT INTO agent_read_cursors (
                owner_user_id, room_id, reader_id, through_message_id,
                through_message_created_at_unix_ms, updated_at_unix_ms)
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5)
            ON CONFLICT(owner_user_id, room_id, reader_id) DO UPDATE SET
                through_message_id = excluded.through_message_id,
                through_message_created_at_unix_ms = excluded.through_message_created_at_unix_ms,
                updated_at_unix_ms = excluded.updated_at_unix_ms
            WHERE excluded.through_message_created_at_unix_ms >
                    agent_read_cursors.through_message_created_at_unix_ms
               OR (excluded.through_message_created_at_unix_ms =
                    agent_read_cursors.through_message_created_at_unix_ms
                   AND excluded.through_message_id > agent_read_cursors.through_message_id)
            """, ownerUserId, roomId, agentId, messageId, messageCreatedAtUnixMs, Now());
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    private static InboxRow ReadInboxRow(SqliteDataReader reader)
    {
        var message = new MessageRow(reader.GetString(0),
            ParseEnum<AgentMessageSenderKind>(reader.GetString(2)),
            reader.IsDBNull(3) ? null : reader.GetString(3), reader.GetString(4),
            reader.IsDBNull(5) ? null : reader.GetString(5), reader.GetString(6),
            reader.GetInt32(7), reader.GetInt64(8));
        return new InboxRow(reader.GetString(1), message);
    }

    private sealed record MessageCursor(string Id, long CreatedAtUnixMs);
    private sealed record InboxRow(string RoomId, MessageRow Message)
    {
        public string Id => Message.Id;
        public long CreatedAtUnixMs => Message.CreatedAtUnixMs;
    }
}
