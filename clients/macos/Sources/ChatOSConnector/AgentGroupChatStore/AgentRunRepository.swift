import ChatOSCore
import SQLite3

enum AgentRunRepository {
    static func run(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        deliveryID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentGroupChatRun? {
        preparedStatement()
        let values: [String] = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT run_json FROM local_agent_group_chat_runs
            WHERE owner_user_id = ? AND delivery_id = ? LIMIT 1
            """,
            [.text(ownerUserID), .text(deliveryID)]
        ) { string($0, 0) }
        guard let json = values.first else { return nil }
        return try AgentGroupChatRowMapper.run(json)
    }

    static func listUnfinished(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        projectID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentGroupChatRun] {
        preparedStatement()
        let values: [String] = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT run_json FROM local_agent_group_chat_runs
            WHERE owner_user_id = ? AND project_id = ?
              AND status NOT IN ('completed', 'failed')
            ORDER BY updated_at_unix_ms DESC, id DESC LIMIT ?
            """,
            [.text(ownerUserID), .text(projectID), .integer(Int64(limit))]
        ) { string($0, 0) }
        return try values.map(AgentGroupChatRowMapper.run)
    }

    static func listForAgent(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentGroupChatRun] {
        preparedStatement()
        let values: [String] = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT run_json FROM local_agent_group_chat_runs
            WHERE owner_user_id = ? AND agent_id = ?
            ORDER BY updated_at_unix_ms DESC, id DESC LIMIT ?
            """,
            [.text(ownerUserID), .text(agentID), .integer(Int64(limit))]
        ) { string($0, 0) }
        return try values.map(AgentGroupChatRowMapper.run)
    }

    static func listForRoom(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentGroupChatRun] {
        preparedStatement()
        let values: [String] = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT run_json FROM local_agent_group_chat_runs
            WHERE owner_user_id = ? AND room_id = ?
            ORDER BY updated_at_unix_ms DESC, id DESC LIMIT ?
            """,
            [.text(ownerUserID), .text(roomID), .integer(Int64(limit))]
        ) { string($0, 0) }
        return try values.map(AgentGroupChatRowMapper.run)
    }

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }
}
