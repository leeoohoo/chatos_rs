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
                SELECT t.id AS todo_id, t.agent_id,
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
            SELECT \(qualifiedColumns)
            FROM ranked
            JOIN local_agent_todos t
              ON t.owner_user_id = ? AND t.id = ranked.todo_id
            WHERE ranked.ready_position = 1
            ORDER BY t.agent_id
            """,
            [
                .text(ownerUserID), .integer(Int64(limit)), .text(ownerUserID),
                .text(ownerUserID),
            ],
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

}
