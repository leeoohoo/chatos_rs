import ChatOSCore
import Foundation
import SQLite3

enum AgentTodoRepository {
    struct StartState {
        let agentIsActive: Bool
        let todo: LocalAgentTodo?
    }

    struct EventRecipient {
        let eventKey: String
        let todoID: String
        let eventKind: String
        let recipientAgentID: String
        let deliveryID: String
        let messageID: String
    }

    static func insertEventRecipients(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        recipients: [EventRecipient],
        nowUnixMs: Int64,
        preparedStatement: () -> Void
    ) throws {
        guard !recipients.isEmpty else { return }
        let placeholders = Array(repeating: "(?, ?, ?, ?, ?, ?, ?, ?)", count: recipients.count)
            .joined(separator: ",")
        let values = recipients.flatMap { recipient in
            [
                AgentGroupChatDatabase.Value.text(ownerUserID),
                .text(recipient.eventKey),
                .text(recipient.todoID),
                .text(recipient.eventKind),
                .text(recipient.recipientAgentID),
                .text(recipient.deliveryID),
                .text(recipient.messageID),
                .integer(nowUnixMs),
            ]
        }
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            INSERT OR IGNORE INTO local_agent_todo_event_recipients (
                owner_user_id, event_key, todo_id, event_kind, recipient_agent_id,
                delivery_id, message_id, created_at_unix_ms
            ) VALUES \(placeholders)
            """,
            values
        )
    }

    static func markInProgress(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoIDs: [String],
        nowUnixMs: Int64,
        preparedStatement: () -> Void
    ) throws -> Int {
        guard !todoIDs.isEmpty else { return 0 }
        let placeholders = Array(repeating: "?", count: todoIDs.count)
            .joined(separator: ",")
        let values = [.integer(nowUnixMs), .text(ownerUserID)]
            + todoIDs.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            UPDATE local_agent_todos
            SET status = 'in_progress', updated_at_unix_ms = ?
            WHERE owner_user_id = ? AND id IN (\(placeholders)) AND status = 'pending'
            """,
            values
        )
        return Int(sqlite3_changes(handle))
    }

    static func reorderForAgent(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        orderedTodoIDs: [String],
        nowUnixMs: Int64,
        preparedStatement: () -> Void
    ) throws {
        try updateOrdering(
            handle,
            ownerUserID: ownerUserID,
            scopeColumn: "agent_id",
            scopeID: agentID,
            orderedTodoIDs: orderedTodoIDs,
            nowUnixMs: nowUnixMs,
            preparedStatement: preparedStatement
        )
    }

    static func reorderForTeam(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        teamRoomID: String,
        orderedTodoIDs: [String],
        nowUnixMs: Int64,
        preparedStatement: () -> Void
    ) throws {
        try updateOrdering(
            handle,
            ownerUserID: ownerUserID,
            scopeColumn: "team_room_id",
            scopeID: teamRoomID,
            orderedTodoIDs: orderedTodoIDs,
            nowUnixMs: nowUnixMs,
            preparedStatement: preparedStatement
        )
    }

    static func insertDependencies(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoID: String,
        prerequisiteTodoIDs: [String],
        nowUnixMs: Int64,
        preparedStatement: () -> Void
    ) throws {
        guard !prerequisiteTodoIDs.isEmpty else { return }
        let rowPlaceholders = Array(
            repeating: "(?, ?, ?, ?)",
            count: prerequisiteTodoIDs.count
        ).joined(separator: ",")
        let values = prerequisiteTodoIDs.flatMap { prerequisiteTodoID in
            [
                AgentGroupChatDatabase.Value.text(ownerUserID),
                .text(todoID),
                .text(prerequisiteTodoID),
                .integer(nowUnixMs),
            ]
        }
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            INSERT INTO local_agent_todo_dependencies (
                owner_user_id, todo_id, prerequisite_todo_id, created_at_unix_ms
            ) VALUES \(rowPlaceholders)
            """,
            values
        )
    }

    static func insertSources(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoID: String,
        sources: [LocalAgentTodoSourceDraft],
        nowUnixMs: Int64,
        preparedStatement: () -> Void
    ) throws {
        guard !sources.isEmpty else { return }
        let rowPlaceholders = Array(repeating: "(?, ?, ?, ?, ?, ?)", count: sources.count)
            .joined(separator: ",")
        let values = sources.flatMap { source in
            [
                AgentGroupChatDatabase.Value.text(ownerUserID),
                .text(todoID),
                .text(source.roomID),
                .text(source.messageID),
                .text(source.relation.rawValue),
                .integer(nowUnixMs),
            ]
        }
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            INSERT OR IGNORE INTO local_agent_todo_sources (
                owner_user_id, todo_id, conversation_id, message_id, relation,
                created_at_unix_ms
            ) VALUES \(rowPlaceholders)
            """,
            values
        )
    }

    /// Returns at most one ready Todo for each of the first `limit` Agents that own pending work.
    /// Readiness, active membership, and an already occupied executor lane are evaluated by the
    /// same statement so account-wide scheduling does not issue two SELECTs per Agent.
    static func nextReadyBatch(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodo] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            WITH pending_agents AS (
                SELECT DISTINCT t.agent_id
                FROM local_agent_todos t
                JOIN local_agent_profiles a
                  ON a.owner_user_id = t.owner_user_id AND a.id = t.agent_id
                WHERE t.owner_user_id = ? AND t.status = 'pending'
                  AND t.team_room_id IS NOT NULL AND a.status = 'active'
                ORDER BY t.agent_id
                LIMIT ?
            ), ranked AS (
                SELECT \(qualifiedColumns),
                       ROW_NUMBER() OVER (
                         PARTITION BY t.agent_id
                         ORDER BY t.priority DESC, t.sort_order, t.created_at_unix_ms, t.id
                       ) AS ready_position
                FROM local_agent_todos t
                JOIN pending_agents candidate ON candidate.agent_id = t.agent_id
                WHERE t.owner_user_id = ? AND t.status = 'pending'
                  AND EXISTS (
                    SELECT 1 FROM project_agent_rooms r
                    JOIN project_agent_room_members m
                      ON m.owner_user_id = r.owner_user_id AND m.room_id = r.id
                    WHERE r.owner_user_id = t.owner_user_id AND r.id = t.team_room_id
                      AND r.status = 'active' AND m.agent_id = t.agent_id
                      AND m.status = 'active'
                  )
                  AND NOT EXISTS (
                    SELECT 1
                    FROM local_agent_todo_dependencies dependency
                    JOIN local_agent_todos prerequisite
                      ON prerequisite.owner_user_id = dependency.owner_user_id
                     AND prerequisite.id = dependency.prerequisite_todo_id
                    WHERE dependency.owner_user_id = t.owner_user_id
                      AND dependency.todo_id = t.id
                      AND prerequisite.status != 'completed'
                  )
                  AND NOT EXISTS (
                    SELECT 1 FROM project_agent_deliveries delivery
                    WHERE delivery.owner_user_id = t.owner_user_id
                      AND delivery.target_agent_id = t.agent_id
                      AND delivery.trigger_kind = 'todo'
                      AND delivery.status IN ('pending', 'running')
                  )
            )
            SELECT \(columns) FROM ranked
            WHERE ready_position = 1
            ORDER BY agent_id
            """,
            [.text(ownerUserID), .integer(Int64(limit)), .text(ownerUserID)],
            row: AgentGroupChatRowMapper.todo
        )
    }

    static func pendingAgentIDs(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [String] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT DISTINCT t.agent_id
            FROM local_agent_todos t
            JOIN local_agent_profiles a
              ON a.owner_user_id = t.owner_user_id AND a.id = t.agent_id
            WHERE t.owner_user_id = ? AND t.status = 'pending'
              AND t.team_room_id IS NOT NULL AND a.status = 'active'
            ORDER BY t.agent_id
            LIMIT ?
            """,
            [.text(ownerUserID), .integer(Int64(limit))]
        ) { string($0, 0) }
    }

    static func running(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTodo? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(columns) FROM local_agent_todos t
            WHERE t.owner_user_id = ? AND t.agent_id = ? AND t.status = 'in_progress'
            ORDER BY t.updated_at_unix_ms, t.id
            LIMIT 1
            """,
            [.text(ownerUserID), .text(agentID)],
            row: AgentGroupChatRowMapper.todo
        ).first
    }

    static func runningCount(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        preparedStatement: () -> Void
    ) throws -> Int64 {
        preparedStatement()
        return try AgentGroupChatDatabase.scalarInt64(
            handle,
            """
            SELECT COUNT(*) FROM local_agent_todos
            WHERE owner_user_id = ? AND agent_id = ? AND status = 'in_progress'
            """,
            [.text(ownerUserID), .text(agentID)]
        )
    }

    static func nextReady(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTodo? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(columns) FROM local_agent_todos t
            WHERE t.owner_user_id = ? AND t.agent_id = ? AND t.status = 'pending'
              AND EXISTS (
                SELECT 1 FROM project_agent_rooms r
                JOIN project_agent_room_members m
                  ON m.owner_user_id = r.owner_user_id AND m.room_id = r.id
                WHERE r.owner_user_id = t.owner_user_id AND r.id = t.team_room_id
                  AND r.status = 'active' AND m.agent_id = t.agent_id
                  AND m.status = 'active'
              )
              AND NOT EXISTS (
                SELECT 1
                FROM local_agent_todo_dependencies dependency
                JOIN local_agent_todos prerequisite
                  ON prerequisite.owner_user_id = dependency.owner_user_id
                 AND prerequisite.id = dependency.prerequisite_todo_id
                WHERE dependency.owner_user_id = t.owner_user_id
                  AND dependency.todo_id = t.id
                  AND prerequisite.status != 'completed'
              )
            ORDER BY t.priority DESC, t.sort_order, t.created_at_unix_ms, t.id
            LIMIT 1
            """,
            [.text(ownerUserID), .text(agentID)],
            row: AgentGroupChatRowMapper.todo
        ).first
    }

    /// Selects the next executable Todo only when the Agent's executor lane is idle. Keeping the
    /// lane predicates in this statement avoids separate outstanding-delivery and running-Todo
    /// probes while preserving the surrounding transaction's atomic start decision.
    static func startState(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        preparedStatement: () -> Void
    ) throws -> StartState {
        preparedStatement()
        guard let state = try AgentGroupChatDatabase.query(
            handle,
            """
            WITH candidate AS (
                SELECT \(qualifiedColumns) FROM local_agent_todos t
                WHERE t.owner_user_id = ? AND t.agent_id = ? AND t.status = 'pending'
                  AND NOT EXISTS (
                    SELECT 1 FROM local_agent_todos running
                    WHERE running.owner_user_id = t.owner_user_id
                      AND running.agent_id = t.agent_id
                      AND running.status = 'in_progress'
                  )
                  AND NOT EXISTS (
                    SELECT 1 FROM project_agent_deliveries delivery
                    WHERE delivery.owner_user_id = t.owner_user_id
                      AND delivery.target_agent_id = t.agent_id
                      AND delivery.trigger_kind = 'todo'
                      AND delivery.status IN ('pending', 'running')
                  )
                  AND EXISTS (
                    SELECT 1 FROM project_agent_rooms r
                    JOIN project_agent_room_members m
                      ON m.owner_user_id = r.owner_user_id AND m.room_id = r.id
                    WHERE r.owner_user_id = t.owner_user_id AND r.id = t.team_room_id
                      AND r.status = 'active' AND m.agent_id = t.agent_id
                      AND m.status = 'active'
                  )
                  AND NOT EXISTS (
                    SELECT 1
                    FROM local_agent_todo_dependencies dependency
                    JOIN local_agent_todos prerequisite
                      ON prerequisite.owner_user_id = dependency.owner_user_id
                     AND prerequisite.id = dependency.prerequisite_todo_id
                    WHERE dependency.owner_user_id = t.owner_user_id
                      AND dependency.todo_id = t.id
                      AND prerequisite.status != 'completed'
                  )
                ORDER BY t.priority DESC, t.sort_order, t.created_at_unix_ms, t.id
                LIMIT 1
            )
            SELECT \(columns), EXISTS(
                SELECT 1 FROM local_agent_profiles profile
                WHERE profile.owner_user_id = ? AND profile.id = ?
                  AND profile.status = 'active'
            )
            FROM (SELECT 1) anchor
            LEFT JOIN candidate ON 1 = 1
            """,
            [.text(ownerUserID), .text(agentID), .text(ownerUserID), .text(agentID)],
            row: { statement in
                let todo: LocalAgentTodo? = if sqlite3_column_type(statement, 1) == SQLITE_NULL {
                    nil
                } else {
                    try AgentGroupChatRowMapper.todo(statement)
                }
                return StartState(
                    agentIsActive: sqlite3_column_int64(statement, 18) != 0,
                    todo: todo
                )
            }
        ).first else {
            throw AgentGroupChatError.storage("Agent Todo start state is missing")
        }
        return state
    }

    static func dependenciesCreateCycle(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        prerequisiteTodoIDs: [String],
        todoID: String,
        preparedStatement: () -> Void
    ) throws -> Bool {
        let ids = Array(Set(prerequisiteTodoIDs)).sorted()
        guard !ids.isEmpty else { return false }
        let placeholders = Array(repeating: "?", count: ids.count).joined(separator: ",")
        let values = [.text(ownerUserID)]
            + ids.map(AgentGroupChatDatabase.Value.text)
            + [.text(ownerUserID), .text(todoID)]
        preparedStatement()
        return try AgentGroupChatDatabase.scalarInt64(
            handle,
            """
            WITH RECURSIVE ancestors(id) AS (
                SELECT id FROM local_agent_todos
                WHERE owner_user_id = ? AND id IN (\(placeholders))
                UNION
                SELECT dependency.prerequisite_todo_id
                FROM local_agent_todo_dependencies dependency
                JOIN ancestors ON dependency.todo_id = ancestors.id
                WHERE dependency.owner_user_id = ?
            )
            SELECT COUNT(*) FROM ancestors WHERE id = ?
            """,
            values
        ) > 0
    }

    static func incompleteDependencyCount(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoID: String,
        preparedStatement: () -> Void
    ) throws -> Int64 {
        preparedStatement()
        return try AgentGroupChatDatabase.scalarInt64(
            handle,
            """
            SELECT COUNT(*)
            FROM local_agent_todo_dependencies dependency
            JOIN local_agent_todos prerequisite
              ON prerequisite.owner_user_id = dependency.owner_user_id
             AND prerequisite.id = dependency.prerequisite_todo_id
            WHERE dependency.owner_user_id = ? AND dependency.todo_id = ?
              AND prerequisite.status != 'completed'
            """,
            [.text(ownerUserID), .text(todoID)]
        )
    }

    static func ready(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        todoID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTodo? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(qualifiedColumns) FROM local_agent_todos t
            WHERE t.owner_user_id = ? AND t.agent_id = ? AND t.id = ?
              AND t.status = 'pending'
              AND NOT EXISTS (
                SELECT 1
                FROM local_agent_todo_dependencies dependency
                JOIN local_agent_todos prerequisite
                  ON prerequisite.owner_user_id = dependency.owner_user_id
                 AND prerequisite.id = dependency.prerequisite_todo_id
                WHERE dependency.owner_user_id = t.owner_user_id
                  AND dependency.todo_id = t.id
                  AND prerequisite.status != 'completed'
              )
            LIMIT 1
            """,
            [.text(ownerUserID), .text(agentID), .text(todoID)],
            row: AgentGroupChatRowMapper.todo
        ).first
    }

    static func nextProgressSequence(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoID: String,
        preparedStatement: () -> Void
    ) throws -> Int64 {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT COALESCE(MAX(sequence), 0) + 1
            FROM local_agent_todo_events WHERE owner_user_id = ? AND todo_id = ?
            """,
            [.text(ownerUserID), .text(todoID)]
        ) { sqlite3_column_int64($0, 0) }.first ?? 1
    }

    static func nextSortOrder(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        preparedStatement: () -> Void
    ) throws -> Int64 {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM local_agent_todos WHERE owner_user_id = ? AND agent_id = ?",
            [.text(ownerUserID), .text(agentID)]
        ) { sqlite3_column_int64($0, 0) }.first ?? 0
    }

    static func readyDependents(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        prerequisiteTodoID: String,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodo] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(qualifiedColumns)
            FROM local_agent_todo_dependencies dependency
            JOIN local_agent_todos t
              ON t.owner_user_id = dependency.owner_user_id AND t.id = dependency.todo_id
            WHERE dependency.owner_user_id = ? AND dependency.prerequisite_todo_id = ?
              AND t.status = 'pending'
              AND NOT EXISTS (
                SELECT 1
                FROM local_agent_todo_dependencies required
                JOIN local_agent_todos prerequisite
                  ON prerequisite.owner_user_id = required.owner_user_id
                 AND prerequisite.id = required.prerequisite_todo_id
                WHERE required.owner_user_id = t.owner_user_id
                  AND required.todo_id = t.id
                  AND prerequisite.status != 'completed'
              )
            ORDER BY t.priority DESC, t.sort_order, t.id
            """,
            [.text(ownerUserID), .text(prerequisiteTodoID)],
            row: AgentGroupChatRowMapper.todo
        )
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

    /// Resolves the durable `todo:<id>` Delivery identity and Todo in one statement. Keeping the
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
          AND d.deduplication_key = 'todo:' || t.id
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

    private static let columns = "owner_user_id, id, agent_id, team_room_id, source_room_id, source_message_id, title, detail, priority, sort_order, request_key, status, blocked_reason, result, created_at_unix_ms, updated_at_unix_ms, execution_plan_json, execution_contract_json"

    private static let qualifiedColumns = "t.owner_user_id, t.id, t.agent_id, t.team_room_id, t.source_room_id, t.source_message_id, t.title, t.detail, t.priority, t.sort_order, t.request_key, t.status, t.blocked_reason, t.result, t.created_at_unix_ms, t.updated_at_unix_ms, t.execution_plan_json, t.execution_contract_json"

    private static let todoDisplayOrderSQL = """
     ORDER BY CASE status
         WHEN 'in_progress' THEN 0
         WHEN 'pending' THEN 1
         WHEN 'blocked' THEN 2
         WHEN 'completed' THEN 3
         WHEN 'cancelled' THEN 4
         ELSE 5
     END, priority DESC, sort_order, created_at_unix_ms, id
    """

    private static let qualifiedTodoDisplayOrderSQL = """
     ORDER BY CASE t.status
         WHEN 'in_progress' THEN 0
         WHEN 'pending' THEN 1
         WHEN 'blocked' THEN 2
         WHEN 'completed' THEN 3
         WHEN 'cancelled' THEN 4
         ELSE 5
     END, t.priority DESC, t.sort_order, t.created_at_unix_ms, t.id
    """

    private static func updateOrdering(
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
