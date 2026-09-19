import ChatOSCore
import SQLite3

enum AgentTodoRepository {
    static func listForAgent(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        includeTerminal: Bool,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodo] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_todos WHERE owner_user_id = ? AND agent_id = ?"
                + (includeTerminal ? "" : " AND status NOT IN ('completed', 'cancelled')")
                + " ORDER BY priority DESC, sort_order, created_at_unix_ms, id",
            [.text(ownerUserID), .text(agentID)],
            row: AgentGroupChatRowMapper.todo
        )
    }

    static func listForTeam(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        teamRoomID: String,
        includeTerminal: Bool,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodo] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_todos WHERE owner_user_id = ? AND team_room_id = ?"
                + (includeTerminal ? "" : " AND status NOT IN ('completed', 'cancelled')")
                + " ORDER BY priority DESC, sort_order, created_at_unix_ms, id",
            [.text(ownerUserID), .text(teamRoomID)],
            row: AgentGroupChatRowMapper.todo
        )
    }

    static func find(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        todoID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTodo? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_todos WHERE owner_user_id = ? AND agent_id = ? AND id = ? LIMIT 1",
            [.text(ownerUserID), .text(agentID), .text(todoID)],
            row: AgentGroupChatRowMapper.todo
        ).first
    }

    static func find(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        requestKey: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTodo? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_todos WHERE owner_user_id = ? AND agent_id = ? AND request_key = ? LIMIT 1",
            [.text(ownerUserID), .text(agentID), .text(requestKey)],
            row: AgentGroupChatRowMapper.todo
        ).first
    }

    private static let columns = "owner_user_id, id, agent_id, team_room_id, source_room_id, source_message_id, title, detail, priority, sort_order, request_key, status, blocked_reason, result, created_at_unix_ms, updated_at_unix_ms, execution_plan_json, execution_contract_json"
}
