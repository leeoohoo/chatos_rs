import ChatOSCore
import SQLite3

enum AgentMessageRepository {
    private static let columns = "owner_user_id, id, room_id, sender_kind, sender_id, content, reply_to_message_id, source_run_id, causation_id, root_message_id, hop_count, created_at_unix_ms"

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
