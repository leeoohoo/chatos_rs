import ChatOSCore
import Foundation
import SQLite3

extension AgentTodoRepository {
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
            SELECT id, todo_id, sequence, kind, run_id, stage, detail,
                   asset_update_suggestions_json, created_at_unix_ms
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
            let suggestions: [LocalAgentTeamAssetUpdateSuggestion]
            do {
                suggestions = try JSONDecoder().decode(
                    [LocalAgentTeamAssetUpdateSuggestion].self,
                    from: Data(string(statement, 7).utf8)
                )
            } catch {
                throw AgentGroupChatError.storage("invalid team asset update suggestions")
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
                assetUpdateSuggestions: suggestions,
                createdAtUnixMs: sqlite3_column_int64(statement, 8)
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

    static func listDependencies(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoIDs: [String],
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodoDependency] {
        guard !todoIDs.isEmpty else { return [] }
        let placeholders = Array(repeating: "?", count: todoIDs.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + todoIDs.map(AgentGroupChatDatabase.Value.text)
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
            WHERE d.owner_user_id = ? AND d.todo_id IN (\(placeholders))
            ORDER BY d.todo_id, d.created_at_unix_ms, d.prerequisite_todo_id
            """,
            values
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

    static func listSources(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoIDs: [String],
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodoSourceLink] {
        guard !todoIDs.isEmpty else { return [] }
        let placeholders = Array(repeating: "?", count: todoIDs.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + todoIDs.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT todo_id, conversation_id, message_id, relation, created_at_unix_ms
            FROM local_agent_todo_sources
            WHERE owner_user_id = ? AND todo_id IN (\(placeholders))
            ORDER BY todo_id, created_at_unix_ms, message_id, relation
            """,
            values
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
                + todoDisplayOrderSQL,
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
                + todoDisplayOrderSQL,
            [.text(ownerUserID), .text(teamRoomID)],
            row: AgentGroupChatRowMapper.todo
        )
    }

    static func listVisibleTeams(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        includeTerminal: Bool,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodo] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(qualifiedColumns) FROM local_agent_todos t"
                + " JOIN project_agent_rooms room"
                + " ON room.owner_user_id = t.owner_user_id AND room.id = t.team_room_id"
                + " JOIN project_agent_room_members member"
                + " ON member.owner_user_id = t.owner_user_id"
                + " AND member.room_id = t.team_room_id AND member.agent_id = ?"
                + " WHERE t.owner_user_id = ? AND room.status = 'active'"
                + " AND room.conversation_kind = 'project_team' AND member.status = 'active'"
                + (includeTerminal ? "" : " AND t.status NOT IN ('completed', 'cancelled')")
                + qualifiedTodoDisplayOrderSQL,
            [.text(agentID), .text(ownerUserID)],
            row: AgentGroupChatRowMapper.todo
        )
    }

    static func findMany(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoIDs: [String],
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodo] {
        guard !todoIDs.isEmpty else { return [] }
        let placeholders = Array(repeating: "?", count: todoIDs.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + todoIDs.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_todos WHERE owner_user_id = ? AND id IN (\(placeholders))",
            values,
            row: AgentGroupChatRowMapper.todo
        )
    }

    /// Resolves the durable `todo:<id>[:attempt:<n>]` Delivery identity and Todo in one statement. Keeping the
    /// target Agent in the JOIN preserves the old two-read authorization semantics without an
    /// intermediate Delivery allocation or a second SQLite round trip.
    static func forDelivery(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        deliveryID: String,
        requireRunning: Bool,
        agentID: String? = nil,
        roomID: String? = nil,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTodo? {
        var predicate = """
        d.owner_user_id = ? AND d.id = ? AND d.trigger_kind = 'todo'
          AND d.target_agent_id = t.agent_id
          AND (d.deduplication_key = 'todo:' || t.id
               OR d.deduplication_key LIKE 'todo:' || t.id || ':attempt:%')
        """
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(deliveryID)]
        if requireRunning {
            predicate += " AND d.status = 'running' AND t.status = 'in_progress'"
        }
        if let agentID {
            predicate += " AND t.agent_id = ?"
            values.append(.text(agentID))
        }
        if let roomID {
            predicate += " AND t.team_room_id = ? AND d.room_id = ?"
            values.append(contentsOf: [.text(roomID), .text(roomID)])
        }
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(qualifiedColumns)
            FROM local_agent_todos t
            JOIN project_agent_deliveries d ON d.owner_user_id = t.owner_user_id
            WHERE \(predicate)
            LIMIT 1
            """,
            values,
            row: AgentGroupChatRowMapper.todo
        ).first
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

    static let columns = "owner_user_id, id, agent_id, team_room_id, source_room_id, source_message_id, title, detail, priority, sort_order, request_key, status, blocked_reason, result, created_at_unix_ms, updated_at_unix_ms, execution_plan_json, execution_contract_json"

    static let qualifiedColumns = "t.owner_user_id, t.id, t.agent_id, t.team_room_id, t.source_room_id, t.source_message_id, t.title, t.detail, t.priority, t.sort_order, t.request_key, t.status, t.blocked_reason, t.result, t.created_at_unix_ms, t.updated_at_unix_ms, t.execution_plan_json, t.execution_contract_json"

    static let todoDisplayOrderSQL = """
     ORDER BY CASE status
         WHEN 'in_progress' THEN 0
         WHEN 'pending' THEN 1
         WHEN 'blocked' THEN 2
         WHEN 'completed' THEN 3
         WHEN 'cancelled' THEN 4
         ELSE 5
     END, priority DESC, sort_order, created_at_unix_ms, id
    """

    static let qualifiedTodoDisplayOrderSQL = """
     ORDER BY CASE t.status
         WHEN 'in_progress' THEN 0
         WHEN 'pending' THEN 1
         WHEN 'blocked' THEN 2
         WHEN 'completed' THEN 3
         WHEN 'cancelled' THEN 4
         ELSE 5
     END, t.priority DESC, t.sort_order, t.created_at_unix_ms, t.id
    """

    static func updateOrdering(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        scopeColumn: String,
        scopeID: String,
        orderedTodoIDs: [String],
        nowUnixMs: Int64,
        preparedStatement: () -> Void
    ) throws {
        guard !orderedTodoIDs.isEmpty else { return }
        let rows = orderedTodoIDs.enumerated().map { offset, id in
            (
                id: id,
                priority: Int64(max(0, 100 - offset)),
                sortOrder: Int64(offset)
            )
        }
        let placeholders = Array(repeating: "(?, ?, ?)", count: rows.count)
            .joined(separator: ",")
        let values = rows.flatMap { row in
            [
                AgentGroupChatDatabase.Value.text(row.id),
                .integer(row.priority),
                .integer(row.sortOrder),
            ]
        } + [.integer(nowUnixMs), .text(ownerUserID), .text(scopeID)]
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            WITH todo_order(id, priority, sort_order) AS (
                VALUES \(placeholders)
            )
            UPDATE local_agent_todos
            SET priority = (
                    SELECT priority FROM todo_order WHERE todo_order.id = local_agent_todos.id
                ),
                sort_order = (
                    SELECT sort_order FROM todo_order WHERE todo_order.id = local_agent_todos.id
                ),
                updated_at_unix_ms = ?
            WHERE owner_user_id = ? AND \(scopeColumn) = ?
              AND id IN (SELECT id FROM todo_order)
            """,
            values
        )
    }

    static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }

    static func optionalString(_ statement: OpaquePointer, _ index: Int32) -> String? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL,
              let value = sqlite3_column_text(statement, index) else { return nil }
        return String(cString: value)
    }
}
