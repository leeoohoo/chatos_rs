import ChatOSCore
import SQLite3

enum AgentMessageRepository {
    struct Cursor {
        let createdAtUnixMs: Int64
        let messageID: String
    }

    private struct MentionRow {
        let messageID: String
        let agentID: String
    }

    private struct AttachmentRow {
        let messageID: String
        let attachment: ProjectAgentMessageAttachment
    }

    private static let columns = "owner_user_id, id, room_id, sender_kind, sender_id, content, reply_to_message_id, source_run_id, causation_id, root_message_id, hop_count, created_at_unix_ms"

    static func insert(
        _ handle: OpaquePointer?,
        messages: [ProjectAgentMessage],
        preparedStatement: () -> Void
    ) throws {
        guard !messages.isEmpty else { return }
        let placeholders = Array(repeating: "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", count: messages.count)
            .joined(separator: ",")
        let values = messages.flatMap { message in
            [
                AgentGroupChatDatabase.Value.text(message.ownerUserID),
                .text(message.id),
                .text(message.roomID),
                .text(message.senderKind.rawValue),
                .text(message.senderID),
                .text(message.content),
                .optionalText(message.replyToMessageID),
                .optionalText(message.sourceRunID),
                .optionalText(message.causationID),
                .text(message.rootMessageID),
                .integer(Int64(message.hopCount)),
                .integer(message.createdAtUnixMs),
            ]
        }
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            INSERT INTO project_agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_id, content,
                reply_to_message_id, source_run_id, causation_id, root_message_id,
                hop_count, created_at_unix_ms
            ) VALUES \(placeholders)
            """,
            values
        )
    }

    static func insertMentions(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        messageID: String,
        agentIDs: [String],
        preparedStatement: () -> Void
    ) throws {
        guard !agentIDs.isEmpty else { return }
        let placeholders = Array(repeating: "(?, ?, ?, ?)", count: agentIDs.count)
            .joined(separator: ",")
        let values = agentIDs.enumerated().flatMap { position, agentID in
            [
                AgentGroupChatDatabase.Value.text(ownerUserID),
                .text(messageID),
                .text(agentID),
                .integer(Int64(position)),
            ]
        }
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            INSERT INTO project_agent_message_mentions (
                owner_user_id, message_id, agent_id, position
            ) VALUES \(placeholders)
            """,
            values
        )
    }

    static func updateContents(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        updates: [(messageID: String, content: String)],
        preparedStatement: () -> Void
    ) throws -> Int {
        guard !updates.isEmpty else { return 0 }
        let placeholders = Array(repeating: "(?, ?)", count: updates.count)
            .joined(separator: ",")
        let values = updates.flatMap { update in
            [
                AgentGroupChatDatabase.Value.text(update.messageID),
                .text(update.content),
            ]
        } + [.text(ownerUserID)]
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            WITH message_updates(id, content) AS (
                VALUES \(placeholders)
            )
            UPDATE project_agent_messages
            SET content = (
                SELECT content FROM message_updates
                WHERE message_updates.id = project_agent_messages.id
            )
            WHERE owner_user_id = ? AND id IN (SELECT id FROM message_updates)
            """,
            values
        )
        return Int(sqlite3_changes(handle))
    }

    static func list(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        afterUnixMs: Int64?,
        limit: Int,
        preparedStatement: () -> Void,
        row: (OpaquePointer) throws -> ProjectAgentMessage
    ) throws -> [ProjectAgentMessage] {
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(roomID)]
        var predicate = "owner_user_id = ? AND room_id = ? AND NOT (sender_kind = 'system' AND causation_id IN ('heartbeat', 'todo', 'todo_status'))"
        if let afterUnixMs {
            predicate += " AND created_at_unix_ms > ?"
            values.append(.integer(afterUnixMs))
        }
        values.append(.integer(Int64(limit)))
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(columns) FROM project_agent_messages
            WHERE \(predicate) ORDER BY created_at_unix_ms, id LIMIT ?
            """,
            values,
            row: row
        )
    }

    static func find(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        messageID: String,
        preparedStatement: () -> Void,
        row: (OpaquePointer) throws -> ProjectAgentMessage
    ) throws -> ProjectAgentMessage? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM project_agent_messages WHERE owner_user_id = ? AND id = ?",
            [.text(ownerUserID), .text(messageID)],
            row: row
        ).first
    }

    static func findByCausationID(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        causationID: String,
        preparedStatement: () -> Void,
        row: (OpaquePointer) throws -> ProjectAgentMessage
    ) throws -> ProjectAgentMessage? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(columns) FROM project_agent_messages
            WHERE owner_user_id = ? AND room_id = ? AND causation_id = ?
            ORDER BY created_at_unix_ms, id LIMIT 1
            """,
            [.text(ownerUserID), .text(roomID), .text(causationID)],
            row: row
        ).first
    }

    static func pageForward(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        after cursor: Cursor?,
        limit: Int,
        preparedStatement: () -> Void,
        row: (OpaquePointer) throws -> ProjectAgentMessage
    ) throws -> [ProjectAgentMessage] {
        var predicate = "owner_user_id = ? AND room_id = ? AND NOT (sender_kind = 'system' AND causation_id IN ('heartbeat', 'todo', 'todo_status'))"
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(roomID)]
        if let cursor {
            predicate += " AND (created_at_unix_ms > ? OR (created_at_unix_ms = ? AND id > ?))"
            values.append(contentsOf: [
                .integer(cursor.createdAtUnixMs), .integer(cursor.createdAtUnixMs),
                .text(cursor.messageID),
            ])
        }
        values.append(.integer(Int64(limit + 1)))
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(columns) FROM project_agent_messages
            WHERE \(predicate) ORDER BY created_at_unix_ms, id LIMIT ?
            """,
            values,
            row: row
        )
    }

    static func pageBackward(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        before cursor: Cursor?,
        limit: Int,
        preparedStatement: () -> Void,
        row: (OpaquePointer) throws -> ProjectAgentMessage
    ) throws -> [ProjectAgentMessage] {
        var predicate = "owner_user_id = ? AND room_id = ? AND NOT (sender_kind = 'system' AND causation_id IN ('heartbeat', 'todo', 'todo_status'))"
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(roomID)]
        if let cursor {
            predicate += " AND (created_at_unix_ms < ? OR (created_at_unix_ms = ? AND id < ?))"
            values.append(contentsOf: [
                .integer(cursor.createdAtUnixMs), .integer(cursor.createdAtUnixMs),
                .text(cursor.messageID),
            ])
        }
        values.append(.integer(Int64(limit + 1)))
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(columns) FROM project_agent_messages
            WHERE \(predicate) ORDER BY created_at_unix_ms DESC, id DESC LIMIT ?
            """,
            values,
            row: row
        )
    }

    static func listUnread(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        agentID: String,
        after cursor: Cursor?,
        limit: Int,
        preparedStatement: () -> Void,
        row: (OpaquePointer) throws -> ProjectAgentMessage
    ) throws -> [ProjectAgentMessage] {
        var predicate = "owner_user_id = ? AND room_id = ? AND NOT (sender_kind = 'system' AND causation_id IN ('heartbeat', 'todo', 'todo_status'))"
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(roomID)]
        if let cursor {
            predicate += " AND (created_at_unix_ms > ? OR (created_at_unix_ms = ? AND id > ?))"
            values.append(contentsOf: [
                .integer(cursor.createdAtUnixMs), .integer(cursor.createdAtUnixMs),
                .text(cursor.messageID),
            ])
        }
        // An Agent's own persisted replies are transcript context, but never unread work for it.
        predicate += " AND NOT (sender_kind = 'agent' AND sender_id = ?)"
        values.append(.text(agentID))
        values.append(.integer(Int64(limit + 1)))
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(columns) FROM project_agent_messages
            WHERE \(predicate) ORDER BY created_at_unix_ms, id LIMIT ?
            """,
            values,
            row: row
        )
    }

    static func listAllUnread(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        limit: Int,
        preparedStatement: () -> Void,
        row: (OpaquePointer) throws -> ProjectAgentMessage
    ) throws -> [ProjectAgentMessage] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT msg.owner_user_id, msg.id, msg.room_id, msg.sender_kind, msg.sender_id,
                   msg.content, msg.reply_to_message_id, msg.source_run_id,
                   msg.causation_id, msg.root_message_id, msg.hop_count,
                   msg.created_at_unix_ms
            FROM project_agent_messages msg
            JOIN project_agent_rooms room
              ON room.owner_user_id = msg.owner_user_id AND room.id = msg.room_id
            JOIN project_agent_room_members member
              ON member.owner_user_id = msg.owner_user_id
             AND member.room_id = msg.room_id AND member.agent_id = ?
            LEFT JOIN project_agent_read_cursors cursor
              ON cursor.owner_user_id = msg.owner_user_id
             AND cursor.room_id = msg.room_id AND cursor.agent_id = ?
            WHERE msg.owner_user_id = ? AND room.status = 'active'
              AND member.status = 'active'
              AND NOT (msg.sender_kind = 'system' AND msg.causation_id IN ('heartbeat', 'todo', 'todo_status'))
              AND NOT (msg.sender_kind = 'agent' AND msg.sender_id = ?)
              AND (
                cursor.message_id IS NULL
                OR msg.created_at_unix_ms > cursor.message_created_at_unix_ms
                OR (msg.created_at_unix_ms = cursor.message_created_at_unix_ms
                    AND msg.id > cursor.message_id)
              )
            ORDER BY msg.created_at_unix_ms, msg.id
            LIMIT ?
            """,
            [
                .text(agentID), .text(agentID), .text(ownerUserID), .text(agentID),
                .integer(Int64(limit)),
            ],
            row: row
        )
    }

    static func findMany(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        messageIDs: [String],
        preparedStatement: () -> Void,
        row: (OpaquePointer) throws -> ProjectAgentMessage
    ) throws -> [ProjectAgentMessage] {
        guard !messageIDs.isEmpty else { return [] }
        let placeholders = Array(repeating: "?", count: messageIDs.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + messageIDs.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM project_agent_messages WHERE owner_user_id = ? AND id IN (\(placeholders))",
            values,
            row: row
        )
    }

    /// Reads only the ownership field needed to validate Todo source links. Source validation
    /// must not hydrate message mentions and attachments for every referenced message.
    static func roomIDs(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        messageIDs: [String],
        preparedStatement: () -> Void
    ) throws -> [String: String] {
        let ids = Array(Set(messageIDs)).sorted()
        guard !ids.isEmpty else { return [:] }
        let placeholders = Array(repeating: "?", count: ids.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + ids.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        let rows = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT id, room_id FROM project_agent_messages
            WHERE owner_user_id = ? AND id IN (\(placeholders))
            """,
            values
        ) { statement in
            (messageID: string(statement, 0), roomID: string(statement, 1))
        }
        return Dictionary(uniqueKeysWithValues: rows.map { ($0.messageID, $0.roomID) })
    }

    static func mentions(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        messageID: String,
        preparedStatement: () -> Void
    ) throws -> [String] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT agent_id FROM project_agent_message_mentions
            WHERE owner_user_id = ? AND message_id = ? ORDER BY position
            """,
            [.text(ownerUserID), .text(messageID)]
        ) { string($0, 0) }
    }

    static func attachments(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        messageID: String,
        preparedStatement: () -> Void
    ) throws -> [ProjectAgentMessageAttachment] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT id, name, mime_type, size_bytes, kind, origin, sha256, sync_status,
                   artifact_id, storage_provider, bucket, object_key, remote_view_path,
                   upload_error, synced_at_unix_ms
            FROM project_agent_message_attachments
            WHERE owner_user_id = ? AND message_id = ?
            ORDER BY position
            """,
            [.text(ownerUserID), .text(messageID)],
            row: AgentGroupChatRowMapper.messageAttachment
        )
    }

    /// Hydrates relation-backed message fields for an entire page in two fixed queries. Message
    /// pages used to issue one mentions query and one attachments query per row, which made a
    /// 50-message first paint perform 100 avoidable SQLite round trips.
    static func relations(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        messageIDs: [String],
        preparedStatement: () -> Void
    ) throws -> (
        mentionsByMessageID: [String: [String]],
        attachmentsByMessageID: [String: [ProjectAgentMessageAttachment]]
    ) {
        guard !messageIDs.isEmpty else { return ([:], [:]) }
        let placeholders = Array(repeating: "?", count: messageIDs.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + messageIDs.map(AgentGroupChatDatabase.Value.text)

        preparedStatement()
        let mentionRows = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT message_id, agent_id
            FROM project_agent_message_mentions
            WHERE owner_user_id = ? AND message_id IN (\(placeholders))
            ORDER BY message_id, position
            """,
            values
        ) { statement in
            MentionRow(messageID: string(statement, 0), agentID: string(statement, 1))
        }

        preparedStatement()
        let attachmentRows = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT id, name, mime_type, size_bytes, kind, origin, sha256, sync_status,
                   artifact_id, storage_provider, bucket, object_key, remote_view_path,
                   upload_error, synced_at_unix_ms, message_id
            FROM project_agent_message_attachments
            WHERE owner_user_id = ? AND message_id IN (\(placeholders))
            ORDER BY message_id, position
            """,
            values
        ) { statement in
            AttachmentRow(
                messageID: string(statement, 15),
                attachment: try AgentGroupChatRowMapper.messageAttachment(statement)
            )
        }

        return (
            Dictionary(grouping: mentionRows, by: \.messageID)
                .mapValues { $0.map(\.agentID) },
            Dictionary(grouping: attachmentRows, by: \.messageID)
                .mapValues { $0.map(\.attachment) }
        )
    }

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }
}
