import ChatOSCore
import SQLite3

enum AgentProfileRepository {
    struct DueHeartbeatAgent {
        let id: String
        let intervalSeconds: Int64
        let prompt: String
        let scheduledAtUnixMs: Int64
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
            SELECT id, heartbeat_interval_seconds, heartbeat_prompt,
                   next_heartbeat_at_unix_ms
            FROM local_agent_profiles
            WHERE owner_user_id = ? AND status = 'active' AND heartbeat_enabled = 1
              AND next_heartbeat_at_unix_ms IS NOT NULL
              AND next_heartbeat_at_unix_ms <= ?
            ORDER BY next_heartbeat_at_unix_ms, id
            LIMIT ?
            """,
            [.text(ownerUserID), .integer(nowUnixMs), .integer(Int64(limit))]
        ) { statement in
            DueHeartbeatAgent(
                id: string(statement, 0),
                intervalSeconds: sqlite3_column_int64(statement, 1),
                prompt: string(statement, 2),
                scheduledAtUnixMs: sqlite3_column_int64(statement, 3)
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

    private static let columns = "owner_user_id, id, name, description, role_prompt, model_config_id, thinking_level, profession_key, default_plugin_ids_json, default_skill_ids_json, heartbeat_enabled, heartbeat_interval_seconds, heartbeat_prompt, last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms, status, created_at_unix_ms, updated_at_unix_ms"

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }
}
