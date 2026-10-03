import ChatOSCore
import SQLite3

enum AgentDeliveryRepository {
    static func insert(
        _ handle: OpaquePointer?,
        deliveries: [ProjectAgentDelivery],
        preparedStatement: () -> Void
    ) throws {
        guard !deliveries.isEmpty else { return }
        let placeholders = Array(
            repeating: "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            count: deliveries.count
        ).joined(separator: ",")
        let values = deliveries.flatMap { delivery in
            [
                AgentGroupChatDatabase.Value.text(delivery.ownerUserID),
                .text(delivery.id),
                .text(delivery.roomID),
                .text(delivery.messageID),
                .text(delivery.rootMessageID),
                .text(delivery.targetAgentID),
                .text(delivery.triggerKind.rawValue),
                .text(delivery.status.rawValue),
                .integer(Int64(delivery.attempt)),
                .integer(Int64(delivery.hopCount)),
                .text(delivery.deduplicationKey),
                .optionalText(delivery.responseMessageID),
                .optionalText(delivery.lastError),
                delivery.claimedAtUnixMs.map(AgentGroupChatDatabase.Value.integer) ?? .null,
                delivery.completedAtUnixMs.map(AgentGroupChatDatabase.Value.integer) ?? .null,
                .integer(delivery.createdAtUnixMs),
            ]
        }
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            INSERT INTO project_agent_deliveries (
                owner_user_id, id, room_id, message_id, root_message_id,
                target_agent_id, trigger_kind, status, attempt, hop_count,
                deduplication_key, response_message_id, last_error,
                claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
            ) VALUES \(placeholders)
            """,
            values
        )
    }

    static func reactivateFailed(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        deliveryIDs: [String],
        preparedStatement: () -> Void
    ) throws -> Int {
        guard !deliveryIDs.isEmpty else { return 0 }
        let placeholders = Array(repeating: "?", count: deliveryIDs.count)
            .joined(separator: ",")
        let values = [.text(ownerUserID)]
            + deliveryIDs.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            UPDATE project_agent_deliveries
            SET status = 'pending', response_message_id = NULL, last_error = NULL,
                claimed_at_unix_ms = NULL, completed_at_unix_ms = NULL
            WHERE owner_user_id = ? AND id IN (\(placeholders)) AND status = 'failed'
            """,
            values
        )
        return Int(sqlite3_changes(handle))
    }

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

    static func deliveries(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        deduplicationKeys: [String],
        preparedStatement: () -> Void
    ) throws -> [ProjectAgentDelivery] {
        let keys = Array(Set(deduplicationKeys)).sorted()
        guard !keys.isEmpty else { return [] }
        let placeholders = Array(repeating: "?", count: keys.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + keys.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM project_agent_deliveries WHERE owner_user_id = ? AND deduplication_key IN (\(placeholders))",
            values,
            row: AgentGroupChatRowMapper.delivery
        )
    }

    static func nextPendingDeliveryID(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        roomID: String?,
        lane: LocalAgentRunLane?,
        preparedStatement: () -> Void
    ) throws -> String? {
        var sql = """
            SELECT d.id FROM project_agent_deliveries d
            JOIN project_agent_rooms r
              ON r.owner_user_id = d.owner_user_id AND r.id = d.room_id
            JOIN project_agent_room_members m
              ON m.owner_user_id = d.owner_user_id
             AND m.room_id = d.room_id AND m.agent_id = d.target_agent_id
            WHERE d.owner_user_id = ? AND d.target_agent_id = ?
              AND d.status = 'pending' AND r.status = 'active' AND m.status = 'active'
              AND NOT EXISTS (
                SELECT 1 FROM project_agent_deliveries active
                WHERE active.owner_user_id = d.owner_user_id
                  AND active.target_agent_id = d.target_agent_id
                  AND active.status = 'running'
                  AND (
                    (d.trigger_kind = 'todo' AND active.trigger_kind = 'todo')
                    OR
                    (d.trigger_kind != 'todo' AND active.trigger_kind != 'todo')
                  )
              )
            """
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(agentID)]
        if let roomID {
            sql += " AND d.room_id = ?"
            values.append(.text(roomID))
        }
        if let lane {
            switch lane {
            case .manager:
                sql += " AND d.trigger_kind != 'todo'"
            case .executor:
                sql += " AND d.trigger_kind = 'todo'"
            }
        }
        sql += " ORDER BY d.created_at_unix_ms, d.id LIMIT 1"
        preparedStatement()
        return try AgentGroupChatDatabase.query(handle, sql, values) {
            string($0, 0)
        }.first
    }

    static func outstandingCount(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        targetAgentID: String,
        triggerKind: ProjectAgentDeliveryTriggerKind,
        preparedStatement: () -> Void
    ) throws -> Int64 {
        preparedStatement()
        return try AgentGroupChatDatabase.scalarInt64(
            handle,
            """
            SELECT COUNT(*) FROM project_agent_deliveries
            WHERE owner_user_id = ? AND target_agent_id = ?
              AND trigger_kind = '\(triggerKind.rawValue)' AND status IN ('pending', 'running')
            """,
            [.text(ownerUserID), .text(targetAgentID)]
        )
    }

    static func hasOutstandingDeliveries(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        preparedStatement: () -> Void
    ) throws -> Bool {
        preparedStatement()
        return try AgentGroupChatDatabase.scalarInt64(
            handle,
            """
            SELECT EXISTS(
                SELECT 1 FROM project_agent_deliveries
                WHERE owner_user_id = ? AND status IN ('pending', 'running')
                LIMIT 1
            )
            """,
            [.text(ownerUserID)]
        ) == 1
    }

    static func hasPendingDeliveries(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String?,
        lane: LocalAgentRunLane?,
        preparedStatement: () -> Void
    ) throws -> Bool {
        var sql = """
            SELECT EXISTS(
                SELECT 1 FROM project_agent_deliveries d
                JOIN project_agent_rooms r
                  ON r.owner_user_id = d.owner_user_id AND r.id = d.room_id
                JOIN project_agent_room_members m
                  ON m.owner_user_id = d.owner_user_id
                 AND m.room_id = d.room_id AND m.agent_id = d.target_agent_id
                WHERE d.owner_user_id = ? AND d.status = 'pending'
                  AND r.status = 'active' AND m.status = 'active'
            """
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID)]
        if let roomID {
            sql += " AND d.room_id = ?"
            values.append(.text(roomID))
        }
        if let lane {
            switch lane {
            case .manager:
                sql += " AND d.trigger_kind != 'todo'"
            case .executor:
                sql += " AND d.trigger_kind = 'todo'"
            }
        }
        sql += " LIMIT 1)"
        preparedStatement()
        return try AgentGroupChatDatabase.scalarInt64(handle, sql, values) == 1
    }

    static func countForRootMessage(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        rootMessageID: String,
        preparedStatement: () -> Void
    ) throws -> Int64 {
        preparedStatement()
        return try AgentGroupChatDatabase.scalarInt64(
            handle,
            """
            SELECT COUNT(*) FROM project_agent_deliveries
            WHERE owner_user_id = ? AND root_message_id = ?
            """,
            [.text(ownerUserID), .text(rootMessageID)]
        )
    }

    static func outstandingDeliveryIDs(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        preparedStatement: () -> Void
    ) throws -> [String] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT id FROM project_agent_deliveries
            WHERE owner_user_id = ? AND room_id = ? AND status IN ('pending', 'running')
            ORDER BY created_at_unix_ms, id
            """,
            [.text(ownerUserID), .text(roomID)]
        ) { string($0, 0) }
    }

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }

    private static let columns = "owner_user_id, id, room_id, message_id, root_message_id, target_agent_id, trigger_kind, status, attempt, hop_count, deduplication_key, response_message_id, last_error, claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms"
}
