import ChatOSAgentRuntime
import ChatOSCore
import Foundation
import SQLite3

enum AgentRunRepository {
    struct FailedRunUpdate {
        let id: UUID
        let json: String
        let updatedAtUnixMs: Int64
    }

    static func markFailed(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        updates: [FailedRunUpdate],
        preparedStatement: () -> Void
    ) throws -> Int {
        guard !updates.isEmpty else { return 0 }
        let placeholders = Array(repeating: "(?, ?, ?)", count: updates.count)
            .joined(separator: ",")
        let values = updates.flatMap { update in
            [
                AgentGroupChatDatabase.Value.text(update.id.uuidString.lowercased()),
                .text(update.json),
                .integer(update.updatedAtUnixMs),
            ]
        } + [.text(ownerUserID)]
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            WITH run_updates(id, run_json, updated_at_unix_ms) AS (
                VALUES \(placeholders)
            )
            UPDATE local_agent_group_chat_runs
            SET status = 'failed',
                run_json = (
                    SELECT run_json FROM run_updates
                    WHERE run_updates.id = local_agent_group_chat_runs.id
                ),
                updated_at_unix_ms = (
                    SELECT updated_at_unix_ms FROM run_updates
                    WHERE run_updates.id = local_agent_group_chat_runs.id
                )
            WHERE owner_user_id = ? AND id IN (SELECT id FROM run_updates)
            """,
            values
        )
        return Int(sqlite3_changes(handle))
    }

    static func run(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        runID: UUID,
        preparedStatement: () -> Void
    ) throws -> LocalAgentGroupChatRun? {
        preparedStatement()
        let values: [String] = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT run_json FROM local_agent_group_chat_runs
            WHERE owner_user_id = ? AND id = ? LIMIT 1
            """,
            [.text(ownerUserID), .text(runID.uuidString.lowercased())]
        ) { string($0, 0) }
        guard let json = values.first else { return nil }
        return try AgentGroupChatRowMapper.run(json)
    }

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

    /// Loads Runs whose durable Delivery is still pending or running in one room. A room-wide
    /// stop must update every matching Run, but reading them must remain constant-query as the
    /// number of active Agents grows.
    static func listForOutstandingDeliveries(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentGroupChatRun] {
        preparedStatement()
        let values: [String] = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT run.run_json
            FROM local_agent_group_chat_runs run
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = run.owner_user_id
             AND delivery.id = run.delivery_id
            WHERE run.owner_user_id = ? AND delivery.room_id = ?
              AND delivery.status IN ('pending', 'running')
            ORDER BY run.id
            """,
            [.text(ownerUserID), .text(roomID)]
        ) { string($0, 0) }
        return try values.map(AgentGroupChatRowMapper.run)
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

    /// Matches the former recovery scan semantics in one statement: only the latest twenty Runs
    /// per active Agent are considered, and only those whose durable Delivery is still running
    /// become candidates. Agent ordering remains identical to `listAgents`.
    static func listInterrupted(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentGroupChatRun] {
        preparedStatement()
        let values: [String] = try AgentGroupChatDatabase.query(
            handle,
            """
            WITH ranked AS (
                SELECT run.run_json, run.delivery_id, run.agent_id,
                       run.updated_at_unix_ms, run.id, agent.name,
                       ROW_NUMBER() OVER (
                           PARTITION BY run.agent_id
                           ORDER BY run.updated_at_unix_ms DESC, run.id DESC
                       ) AS agent_rank
                FROM local_agent_group_chat_runs run
                JOIN local_agent_profiles agent
                  ON agent.owner_user_id = run.owner_user_id
                 AND agent.id = run.agent_id
                WHERE run.owner_user_id = ? AND agent.status = 'active'
            )
            SELECT ranked.run_json
            FROM ranked
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = ? AND delivery.id = ranked.delivery_id
            WHERE ranked.agent_rank <= 20 AND delivery.status = 'running'
            ORDER BY ranked.name, ranked.agent_id,
                     ranked.updated_at_unix_ms DESC, ranked.id DESC
            LIMIT ?
            """,
            [.text(ownerUserID), .text(ownerUserID), .integer(Int64(limit))]
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

    /// Projects run-history rows without materializing the large checkpoint and event payloads.
    static func listHistorySummariesForRoom(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentRunHistorySummary] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT run.id, run.agent_id, run.delivery_id, run.project_id, run.room_id,
                   delivery.message_id, delivery.trigger_kind, run.status,
                   COALESCE(json_array_length(run.run_json, '$.events'), 0),
                   COALESCE(json_extract(run.run_json, '$.checkpoint.modelCalls'), 0),
                   json_extract(run.run_json, '$.checkpoint.memory.scope.threadID'),
                   json_extract(run.run_json, '$.checkpoint.stopReason'),
                   (SELECT json_extract(event.value, '$.detail')
                    FROM json_each(run.run_json, '$.events') event
                    WHERE json_extract(event.value, '$.kind') IN ('needs_review', 'resume_failed')
                    ORDER BY CAST(event.key AS INTEGER) DESC LIMIT 1),
                   COALESCE(json_extract(run.run_json, '$.checkpoint.elapsedSeconds'), 0),
                   CASE WHEN length(trim(COALESCE(
                       json_extract(run.run_json, '$.checkpoint.result'),
                       json_extract(run.run_json, '$.checkpoint.completionResult'), ''
                   ))) > 0 THEN 1 ELSE 0 END,
                   run.updated_at_unix_ms
            FROM local_agent_group_chat_runs run
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = run.owner_user_id
             AND delivery.id = run.delivery_id
            WHERE run.owner_user_id = ? AND run.room_id = ?
            ORDER BY run.updated_at_unix_ms DESC, run.id DESC
            LIMIT ?
            """,
            [.text(ownerUserID), .text(roomID), .integer(Int64(limit))]
        ) { try historySummary($0) }
    }

    static func listHistorySummariesForAgent(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentRunHistorySummary] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT run.id, run.agent_id, run.delivery_id, run.project_id, run.room_id,
                   delivery.message_id, delivery.trigger_kind, run.status,
                   COALESCE(json_array_length(run.run_json, '$.events'), 0),
                   COALESCE(json_extract(run.run_json, '$.checkpoint.modelCalls'), 0),
                   json_extract(run.run_json, '$.checkpoint.memory.scope.threadID'),
                   json_extract(run.run_json, '$.checkpoint.stopReason'),
                   (SELECT json_extract(event.value, '$.detail')
                    FROM json_each(run.run_json, '$.events') event
                    WHERE json_extract(event.value, '$.kind') IN ('needs_review', 'resume_failed')
                    ORDER BY CAST(event.key AS INTEGER) DESC LIMIT 1),
                   COALESCE(json_extract(run.run_json, '$.checkpoint.elapsedSeconds'), 0),
                   CASE WHEN length(trim(COALESCE(
                       json_extract(run.run_json, '$.checkpoint.result'),
                       json_extract(run.run_json, '$.checkpoint.completionResult'), ''
                   ))) > 0 THEN 1 ELSE 0 END,
                   run.updated_at_unix_ms
            FROM local_agent_group_chat_runs run
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = run.owner_user_id
             AND delivery.id = run.delivery_id
            WHERE run.owner_user_id = ? AND run.agent_id = ?
            ORDER BY run.updated_at_unix_ms DESC, run.id DESC
            LIMIT ?
            """,
            [.text(ownerUserID), .text(agentID), .integer(Int64(limit))]
        ) { try historySummary($0) }
    }

    /// Projects only the newest Todo Run per Todo. SQLite's JSON functions inspect receipts in
    /// place, avoiding allocation and Codable decoding of historical model messages and events.
    static func listLatestTodoSummariesForRoom(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodoRunSummary] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            WITH todo_runs AS (
                SELECT CASE
                           WHEN instr(substr(delivery.deduplication_key, 6), ':attempt:') > 0
                           THEN substr(
                               substr(delivery.deduplication_key, 6),
                               1,
                               instr(substr(delivery.deduplication_key, 6), ':attempt:') - 1
                           )
                           ELSE substr(delivery.deduplication_key, 6)
                       END AS todo_id,
                       run.id AS run_id, run.status, run.run_json,
                       run.updated_at_unix_ms
                FROM local_agent_group_chat_runs run
                JOIN project_agent_deliveries delivery
                  ON delivery.owner_user_id = run.owner_user_id
                 AND delivery.id = run.delivery_id
                WHERE run.owner_user_id = ? AND run.room_id = ?
                  AND delivery.trigger_kind = 'todo'
                  AND delivery.deduplication_key LIKE 'todo:%'
            ), ranked AS (
                SELECT todo_id, run_id, status, run_json, updated_at_unix_ms,
                       ROW_NUMBER() OVER (
                           PARTITION BY todo_id
                           ORDER BY updated_at_unix_ms DESC, run_id DESC
                       ) AS recency_rank
                FROM todo_runs
            )
            SELECT todo_id, run_id, status,
                   (SELECT COUNT(*)
                    FROM json_each(ranked.run_json, '$.checkpoint.receipts')),
                   COALESCE((
                       SELECT json_group_array(path)
                       FROM (
                           SELECT DISTINCT committed.value AS path
                           FROM json_each(
                               ranked.run_json,
                               '$.checkpoint.receipts'
                           ) receipt
                           JOIN json_tree(
                               CASE
                                   WHEN json_valid(json_extract(receipt.value, '$.content'))
                                   THEN json_extract(receipt.value, '$.content')
                                   ELSE '{}'
                               END
                           ) tree
                             ON tree.key = 'committed_paths' AND tree.type = 'array'
                           JOIN json_each(tree.value) committed
                           WHERE COALESCE(
                               json_extract(receipt.value, '$.isError'), 0
                           ) = 0
                             AND committed.type = 'text'
                           ORDER BY path
                       )
                   ), '[]'),
                   updated_at_unix_ms
            FROM ranked
            WHERE recency_rank = 1
            ORDER BY updated_at_unix_ms DESC, run_id DESC
            LIMIT ?
            """,
            [.text(ownerUserID), .text(roomID), .integer(Int64(limit))]
        ) { statement in
            guard let runID = UUID(uuidString: string(statement, 1)),
                  let status = AgentRunCheckpoint.Status(rawValue: string(statement, 2)),
                  let pathsData = string(statement, 4).data(using: .utf8),
                  let committedPaths = try? JSONDecoder().decode([String].self, from: pathsData)
            else {
                throw AgentGroupChatError.storage("invalid Todo Run summary")
            }
            return LocalAgentTodoRunSummary(
                todoID: string(statement, 0),
                runID: runID,
                status: status,
                receiptCount: Int(sqlite3_column_int64(statement, 3)),
                committedPaths: committedPaths,
                updatedAtUnixMs: sqlite3_column_int64(statement, 5)
            )
        }
    }

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }

    private static func optionalString(_ statement: OpaquePointer, _ index: Int32) -> String? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL else { return nil }
        return string(statement, index)
    }

    private static func historySummary(
        _ statement: OpaquePointer
    ) throws -> LocalAgentRunHistorySummary {
        guard let id = UUID(uuidString: string(statement, 0)),
              let triggerKind = ProjectAgentDeliveryTriggerKind(rawValue: string(statement, 6)),
              let status = AgentRunCheckpoint.Status(rawValue: string(statement, 7)) else {
            throw AgentGroupChatError.storage("invalid Run history summary")
        }
        return LocalAgentRunHistorySummary(
            id: id,
            agentID: string(statement, 1),
            deliveryID: string(statement, 2),
            projectID: string(statement, 3),
            roomID: string(statement, 4),
            triggerMessageID: string(statement, 5),
            lane: triggerKind == .todo ? .executor : .manager,
            triggerKind: triggerKind,
            status: status,
            eventCount: Int(sqlite3_column_int64(statement, 8)),
            modelCalls: Int(sqlite3_column_int64(statement, 9)),
            memoryThreadID: optionalString(statement, 10),
            stopReason: optionalString(statement, 11),
            diagnosticReason: optionalString(statement, 12),
            elapsedSeconds: sqlite3_column_double(statement, 13),
            hasResult: sqlite3_column_int(statement, 14) != 0,
            updatedAtUnixMs: sqlite3_column_int64(statement, 15)
        )
    }
}
