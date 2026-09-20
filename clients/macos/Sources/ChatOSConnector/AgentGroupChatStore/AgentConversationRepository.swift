import ChatOSCore
import SQLite3

enum AgentConversationRepository {
    static func firstActiveMemberID(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        preparedStatement: () -> Void
    ) throws -> String? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT agent_id FROM project_agent_room_members
            WHERE owner_user_id = ? AND room_id = ? AND status = 'active'
            ORDER BY joined_at_unix_ms, agent_id LIMIT 1
            """,
            [.text(ownerUserID), .text(roomID)]
        ) { string($0, 0) }.first
    }

    static func activeMemberIDs(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        preparedStatement: () -> Void
    ) throws -> [String] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT agent_id FROM project_agent_room_members
            WHERE owner_user_id = ? AND room_id = ? AND status = 'active'
            ORDER BY joined_at_unix_ms, agent_id
            """,
            [.text(ownerUserID), .text(roomID)]
        ) { string($0, 0) }
    }

    static func listProjectRooms(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        includeArchived: Bool,
        preparedStatement: () -> Void
    ) throws -> [ProjectAgentRoom] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(roomColumns) FROM project_agent_rooms WHERE owner_user_id = ? AND conversation_kind = 'project_team'"
                + (includeArchived ? "" : " AND status = 'active'")
                + " ORDER BY updated_at_unix_ms DESC, id DESC",
            [.text(ownerUserID)],
            row: AgentGroupChatRowMapper.room
        )
    }

    static func listDirectRooms(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        includeArchived: Bool,
        preparedStatement: () -> Void
    ) throws -> [ProjectAgentRoom] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(roomColumns) FROM project_agent_rooms WHERE owner_user_id = ? AND conversation_kind != 'project_team'"
                + (includeArchived ? "" : " AND status = 'active'")
                + " ORDER BY updated_at_unix_ms DESC, id DESC",
            [.text(ownerUserID)],
            row: AgentGroupChatRowMapper.room
        )
    }

    static func listMembers(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        preparedStatement: () -> Void
    ) throws -> [ProjectAgentRoomMember] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(memberColumns) FROM project_agent_room_members
            WHERE owner_user_id = ? AND room_id = ? AND status = 'active'
            ORDER BY joined_at_unix_ms, agent_id
            """,
            [.text(ownerUserID), .text(roomID)],
            row: AgentGroupChatRowMapper.member
        )
    }

    static func activeProjectRoom(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        projectID: String,
        preparedStatement: () -> Void
    ) throws -> ProjectAgentRoom? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(roomColumns) FROM project_agent_rooms
            WHERE owner_user_id = ? AND project_id = ?
              AND conversation_kind = 'project_team' AND status = 'active' LIMIT 1
            """,
            [.text(ownerUserID), .text(projectID)],
            row: AgentGroupChatRowMapper.room
        ).first
    }

    static func directRoom(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        directKey: String,
        preparedStatement: () -> Void
    ) throws -> ProjectAgentRoom? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(roomColumns) FROM project_agent_rooms
            WHERE owner_user_id = ? AND direct_key = ?
              AND conversation_kind != 'project_team' AND status = 'active' LIMIT 1
            """,
            [.text(ownerUserID), .text(directKey)],
            row: AgentGroupChatRowMapper.room
        ).first
    }

    static func room(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        preparedStatement: () -> Void
    ) throws -> ProjectAgentRoom? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(roomColumns) FROM project_agent_rooms WHERE owner_user_id = ? AND id = ?",
            [.text(ownerUserID), .text(roomID)],
            row: AgentGroupChatRowMapper.room
        ).first
    }

    static func member(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        agentID: String,
        preparedStatement: () -> Void
    ) throws -> ProjectAgentRoomMember? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(memberColumns) FROM project_agent_room_members
            WHERE owner_user_id = ? AND room_id = ? AND agent_id = ?
            """,
            [.text(ownerUserID), .text(roomID), .text(agentID)],
            row: AgentGroupChatRowMapper.member
        ).first
    }

    private static let roomColumns = "owner_user_id, id, project_id, name, goal, default_agent_id, status, created_at_unix_ms, updated_at_unix_ms, conversation_kind, direct_key, project_manager_agent_id"
    private static let memberColumns = "owner_user_id, room_id, agent_id, role, responsibility, plugin_allowlist_json, status, joined_at_unix_ms"

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }
}
