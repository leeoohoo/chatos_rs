import ChatOSCore
import SQLite3

enum AgentTodoRepository {
    static func listSources(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoID: String,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodoSourceLink] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT todo_id, conversation_id, message_id, relation, created_at_unix_ms
            FROM local_agent_todo_sources
            WHERE owner_user_id = ? AND todo_id = ?
            ORDER BY created_at_unix_ms, message_id, relation
            """,
            [.text(ownerUserID), .text(todoID)]
        ) { statement in
            guard let relation = LocalAgentTodoSourceRelation(
                rawValue: string(statement, 3)
            ) else { throw AgentGroupChatError.storage("invalid Agent Todo source relation") }
            return .init(
                todoID: string(statement, 0),
                conversationID: string(statement, 1),
                messageID: string(statement, 2),
                relation: relation,
                createdAtUnixMs: sqlite3_column_int64(statement, 4)
            )
        }
    }

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

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }
}
