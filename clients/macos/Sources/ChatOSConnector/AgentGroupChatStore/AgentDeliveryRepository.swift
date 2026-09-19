import ChatOSCore
import SQLite3

enum AgentDeliveryRepository {
    static func delivery(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        deliveryID: String,
        preparedStatement: () -> Void
    ) throws -> ProjectAgentDelivery? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM project_agent_deliveries WHERE owner_user_id = ? AND id = ?",
            [.text(ownerUserID), .text(deliveryID)],
            row: AgentGroupChatRowMapper.delivery
        ).first
    }

    static func delivery(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        deduplicationKey: String,
        preparedStatement: () -> Void
    ) throws -> ProjectAgentDelivery? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM project_agent_deliveries WHERE owner_user_id = ? AND deduplication_key = ? LIMIT 1",
            [.text(ownerUserID), .text(deduplicationKey)],
            row: AgentGroupChatRowMapper.delivery
        ).first
    }

    static func deliveries(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        deliveryIDs: [String],
        preparedStatement: () -> Void
    ) throws -> [ProjectAgentDelivery] {
        let placeholders = Array(repeating: "?", count: deliveryIDs.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + deliveryIDs.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM project_agent_deliveries WHERE owner_user_id = ? AND id IN (\(placeholders))",
            values,
            row: AgentGroupChatRowMapper.delivery
        )
    }

    private static let columns = "owner_user_id, id, room_id, message_id, root_message_id, target_agent_id, trigger_kind, status, attempt, hop_count, deduplication_key, response_message_id, last_error, claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms"
}
