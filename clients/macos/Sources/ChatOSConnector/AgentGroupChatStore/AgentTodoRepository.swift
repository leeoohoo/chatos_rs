import ChatOSCore
import SQLite3

enum AgentTodoRepository {
    static func pendingDependents(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        prerequisiteTodoID: String,
        preparedStatement: () -> Void
    ) throws -> [(agentID: String, todoID: String)] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT todo.agent_id, todo.id
            FROM local_agent_todo_dependencies dependency
            JOIN local_agent_todos todo
              ON todo.owner_user_id = dependency.owner_user_id AND todo.id = dependency.todo_id
            WHERE dependency.owner_user_id = ? AND dependency.prerequisite_todo_id = ?
              AND todo.status = 'pending'
            ORDER BY todo.priority DESC, todo.sort_order, todo.id
            """,
            [.text(ownerUserID), .text(prerequisiteTodoID)]
        ) { statement in
            (string(statement, 0), string(statement, 1))
        }
    }

    static func listProgress(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        todoID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodoProgress] {
        preparedStatement()
        let newestFirst: [LocalAgentTodoProgress] = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT id, todo_id, sequence, kind, run_id, stage, detail, created_at_unix_ms
            FROM local_agent_todo_events
            WHERE owner_user_id = ? AND todo_id = ?
            ORDER BY sequence DESC
            LIMIT ?
            """,
            [.text(ownerUserID), .text(todoID), .integer(Int64(limit))]
        ) { statement in
            guard let kind = LocalAgentTodoProgressKind(rawValue: string(statement, 3)) else {
                throw AgentGroupChatError.storage("invalid Agent Todo progress kind")
            }
            return .init(
                id: string(statement, 0),
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: string(statement, 1),
                sequence: sqlite3_column_int64(statement, 2),
                kind: kind,
                runID: optionalString(statement, 4),
                stage: string(statement, 5),
                detail: string(statement, 6),
                createdAtUnixMs: sqlite3_column_int64(statement, 7)
            )
        }
        return Array(newestFirst.reversed())
    }

    static func listDependencies(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoID: String,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodoDependency] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT d.todo_id, d.prerequisite_todo_id, prerequisite.agent_id,
                   d.created_at_unix_ms
            FROM local_agent_todo_dependencies d
            JOIN local_agent_todos prerequisite
              ON prerequisite.owner_user_id = d.owner_user_id
             AND prerequisite.id = d.prerequisite_todo_id
            WHERE d.owner_user_id = ? AND d.todo_id = ?
            ORDER BY d.created_at_unix_ms, d.prerequisite_todo_id
            """,
            [.text(ownerUserID), .text(todoID)]
        ) { statement in
            .init(
                todoID: string(statement, 0),
                prerequisiteTodoID: string(statement, 1),
                prerequisiteAgentID: string(statement, 2),
                createdAtUnixMs: sqlite3_column_int64(statement, 3)
            )
        }
    }

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

    private static func optionalString(_ statement: OpaquePointer, _ index: Int32) -> String? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL,
              let value = sqlite3_column_text(statement, index) else { return nil }
        return String(cString: value)
    }
}
