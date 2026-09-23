import ChatOSCore
import SQLite3

enum AgentMessageRepository {
    struct Cursor {
        let createdAtUnixMs: Int64
        let messageID: String
    }

    private static let columns = "owner_user_id, id, room_id, sender_kind, sender_id, content, reply_to_message_id, source_run_id, causation_id, root_message_id, hop_count, created_at_unix_ms"

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

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }
}
