import ChatOSCore
import SQLite3

enum AgentMessageRepository {
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
}
