import ChatOSCore
import SQLite3

enum AgentProfileRepository {
    struct DueHeartbeatAgent {
        let id: String
        let name: String
        let description: String
        let intervalSeconds: Int64
        let prompt: String
        let scheduledAtUnixMs: Int64
        let directRoomID: String?
        let hasOutstandingHeartbeat: Bool
    }

    static func updateHeartbeatSchedule(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        schedules: [(agentID: String, nextAtUnixMs: Int64)],
        nowUnixMs: Int64,
        preparedStatement: () -> Void
    ) throws {
        guard !schedules.isEmpty else { return }
        let placeholders = Array(repeating: "(?, ?)", count: schedules.count)
            .joined(separator: ",")
        let values = schedules.flatMap { schedule in
            [
                AgentGroupChatDatabase.Value.text(schedule.agentID),
                .integer(schedule.nextAtUnixMs),
            ]
        } + [.integer(nowUnixMs), .text(ownerUserID)]
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            WITH heartbeat_schedule(agent_id, next_at_unix_ms) AS (
                VALUES \(placeholders)
            )
            UPDATE local_agent_profiles
            SET last_heartbeat_at_unix_ms = ?,
                next_heartbeat_at_unix_ms = (
                    SELECT next_at_unix_ms FROM heartbeat_schedule
                    WHERE heartbeat_schedule.agent_id = local_agent_profiles.id
                )
            WHERE owner_user_id = ? AND status = 'active' AND heartbeat_enabled = 1
              AND id IN (SELECT agent_id FROM heartbeat_schedule)
            """,
            values
        )
    }

    static func dueHeartbeatAgents(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        nowUnixMs: Int64,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [DueHeartbeatAgent] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT profile.id, profile.name, profile.description,
                   profile.heartbeat_interval_seconds, profile.heartbeat_prompt,
                   profile.next_heartbeat_at_unix_ms,
                   (
                     SELECT room.id FROM project_agent_rooms room
                     WHERE room.owner_user_id = profile.owner_user_id
                       AND room.direct_key = 'human:' || profile.owner_user_id || '|agent:' || profile.id
                       AND room.conversation_kind != 'project_team' AND room.status = 'active'
                     LIMIT 1
                   ),
                   EXISTS(
                     SELECT 1 FROM project_agent_deliveries delivery
                     WHERE delivery.owner_user_id = profile.owner_user_id
                       AND delivery.target_agent_id = profile.id
                       AND delivery.trigger_kind = 'heartbeat'
                       AND delivery.status IN ('pending', 'running')
                     LIMIT 1
                   )
            FROM local_agent_profiles profile
            WHERE profile.owner_user_id = ? AND profile.status = 'active'
              AND profile.heartbeat_enabled = 1
              AND profile.next_heartbeat_at_unix_ms IS NOT NULL
              AND profile.next_heartbeat_at_unix_ms <= ?
            ORDER BY profile.next_heartbeat_at_unix_ms, profile.id
            LIMIT ?
            """,
            [.text(ownerUserID), .integer(nowUnixMs), .integer(Int64(limit))]
        ) { statement in
            DueHeartbeatAgent(
                id: string(statement, 0),
                name: string(statement, 1),
                description: string(statement, 2),
                intervalSeconds: sqlite3_column_int64(statement, 3),
                prompt: string(statement, 4),
                scheduledAtUnixMs: sqlite3_column_int64(statement, 5),
                directRoomID: optionalString(statement, 6),
                hasOutstandingHeartbeat: sqlite3_column_int64(statement, 7) == 1
            )
        }
    }

    static func nextHeartbeatDue(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        preparedStatement: () -> Void
    ) throws -> Int64? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT next_heartbeat_at_unix_ms
            FROM local_agent_profiles
            WHERE owner_user_id = ? AND status = 'active' AND heartbeat_enabled = 1
              AND next_heartbeat_at_unix_ms IS NOT NULL
            ORDER BY next_heartbeat_at_unix_ms
            LIMIT 1
            """,
            [.text(ownerUserID)]
        ) { sqlite3_column_int64($0, 0) }.first
    }

    static func list(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        includeArchived: Bool,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentProfile] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_profiles WHERE owner_user_id = ?"
                + (includeArchived ? "" : " AND status = 'active'")
                + " ORDER BY name, id",
            [.text(ownerUserID)],
            row: AgentGroupChatRowMapper.agent
        )
    }

    static func find(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentProfile? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_profiles WHERE owner_user_id = ? AND id = ?",
            [.text(ownerUserID), .text(agentID)],
            row: AgentGroupChatRowMapper.agent
        ).first
    }

    static func findMany(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentIDs: [String],
        preparedStatement: () -> Void
    ) throws -> [LocalAgentProfile] {
        let ids = Array(Set(agentIDs)).sorted()
        guard !ids.isEmpty else { return [] }
        let placeholders = Array(repeating: "?", count: ids.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + ids.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_profiles WHERE owner_user_id = ? AND id IN (\(placeholders))",
            values,
            row: AgentGroupChatRowMapper.agent
        )
    }

    private static let columns = "owner_user_id, id, name, description, role_prompt, model_config_id, thinking_level, profession_key, default_plugin_ids_json, default_skill_ids_json, heartbeat_enabled, heartbeat_interval_seconds, heartbeat_prompt, last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms, status, created_at_unix_ms, updated_at_unix_ms, avatar_data"

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
