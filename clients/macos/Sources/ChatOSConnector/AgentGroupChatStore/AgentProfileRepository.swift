import ChatOSCore
import SQLite3

enum AgentProfileRepository {
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
}
