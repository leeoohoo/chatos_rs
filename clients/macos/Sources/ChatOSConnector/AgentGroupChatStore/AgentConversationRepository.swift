import ChatOSCore
import Foundation
import SQLite3

enum AgentConversationRepository {
    static func insertRooms(
        _ handle: OpaquePointer?,
        rooms: [ProjectAgentRoom],
        preparedStatement: () -> Void
    ) throws {
        for start in stride(from: 0, to: rooms.count, by: 64) {
            let batch = rooms[start..<min(start + 64, rooms.count)]
            let placeholders = Array(
                repeating: "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                count: batch.count
            ).joined(separator: ",")
            let values = batch.flatMap { room in
                [
                    AgentGroupChatDatabase.Value.text(room.ownerUserID),
                    .text(room.id),
                    .text(room.projectID),
                    .text(room.draft.name),
                    .text(room.draft.goal),
                    .optionalText(room.defaultAgentID),
                    .text(room.status.rawValue),
                    .integer(room.createdAtUnixMs),
                    .integer(room.updatedAtUnixMs),
                    .text(room.conversationKind.rawValue),
                    .optionalText(room.directKey),
                    .optionalText(room.projectManagerAgentID),
                ]
            }
            preparedStatement()
            try AgentGroupChatDatabase.execute(
                handle,
                """
                INSERT INTO project_agent_rooms (
                    owner_user_id, id, project_id, name, goal, default_agent_id, status,
                    created_at_unix_ms, updated_at_unix_ms, conversation_kind, direct_key,
                    project_manager_agent_id
                ) VALUES \(placeholders)
                """,
                values
            )
        }
    }

    static func insertMembers(
        _ handle: OpaquePointer?,
        members: [ProjectAgentRoomMember],
        preparedStatement: () -> Void
    ) throws {
        let encoder = JSONEncoder()
        for start in stride(from: 0, to: members.count, by: 64) {
            let batch = members[start..<min(start + 64, members.count)]
            let values = try batch.flatMap { member in
                [
                    AgentGroupChatDatabase.Value.text(member.ownerUserID),
                    .text(member.roomID),
                    .text(member.agentID),
                    .text(member.draft.role),
                    .text(member.draft.responsibility),
                    .text(String(decoding: try encoder.encode(
                        member.draft.pluginAllowlist
                    ), as: UTF8.self)),
                    .text(member.status.rawValue),
                    .integer(member.joinedAtUnixMs),
                ]
            }
            let placeholders = Array(
                repeating: "(?, ?, ?, ?, ?, ?, ?, ?)",
                count: batch.count
            ).joined(separator: ",")
            preparedStatement()
            try AgentGroupChatDatabase.execute(
                handle,
                """
                INSERT INTO project_agent_room_members (
                    owner_user_id, room_id, agent_id, role, responsibility,
                    plugin_allowlist_json, status, joined_at_unix_ms
                ) VALUES \(placeholders)
                """,
                values
            )
        }
    }

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

    static func listActiveMembers(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        preparedStatement: () -> Void
    ) throws -> [ProjectAgentRoomMember] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT member.owner_user_id, member.room_id, member.agent_id, member.role,
                   member.responsibility, member.plugin_allowlist_json, member.status,
                   member.joined_at_unix_ms
            FROM project_agent_room_members member
            JOIN project_agent_rooms room
              ON room.owner_user_id = member.owner_user_id AND room.id = member.room_id
            WHERE member.owner_user_id = ? AND member.status = 'active'
              AND room.status = 'active'
            ORDER BY member.room_id, member.joined_at_unix_ms, member.agent_id
            """,
            [.text(ownerUserID)],
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

    static func directRooms(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        directKeys: [String],
        preparedStatement: () -> Void
    ) throws -> [ProjectAgentRoom] {
        let keys = Array(Set(directKeys)).sorted()
        guard !keys.isEmpty else { return [] }
        let placeholders = Array(repeating: "?", count: keys.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + keys.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(roomColumns) FROM project_agent_rooms
            WHERE owner_user_id = ? AND direct_key IN (\(placeholders))
              AND conversation_kind = 'human_agent_direct' AND status = 'active'
            """,
            values,
            row: AgentGroupChatRowMapper.room
        )
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

    static func rooms(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomIDs: [String],
        preparedStatement: () -> Void
    ) throws -> [ProjectAgentRoom] {
        let ids = Array(Set(roomIDs)).sorted()
        guard !ids.isEmpty else { return [] }
        let placeholders = Array(repeating: "?", count: ids.count).joined(separator: ",")
        let values = [.text(ownerUserID)] + ids.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(roomColumns) FROM project_agent_rooms WHERE owner_user_id = ? AND id IN (\(placeholders))",
            values,
            row: AgentGroupChatRowMapper.room
        )
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

    /// Returns the requested rooms in which one Agent is currently an active member. Todo source
    /// authorization can then validate a whole set of context rooms without one member SELECT per
    /// source message.
    static func activeMemberRoomIDs(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        agentID: String,
        roomIDs: [String],
        preparedStatement: () -> Void
    ) throws -> Set<String> {
        let ids = Array(Set(roomIDs)).sorted()
        guard !ids.isEmpty else { return [] }
        let placeholders = Array(repeating: "?", count: ids.count).joined(separator: ",")
        let values = [.text(ownerUserID), .text(agentID)]
            + ids.map(AgentGroupChatDatabase.Value.text)
        preparedStatement()
        let rows = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT room_id FROM project_agent_room_members
            WHERE owner_user_id = ? AND agent_id = ? AND status = 'active'
              AND room_id IN (\(placeholders))
            """,
            values
        ) { string($0, 0) }
        return Set(rows)
    }

    private static let roomColumns = "owner_user_id, id, project_id, name, goal, default_agent_id, status, created_at_unix_ms, updated_at_unix_ms, conversation_kind, direct_key, project_manager_agent_id"
    private static let memberColumns = "owner_user_id, room_id, agent_id, role, responsibility, plugin_allowlist_json, status, joined_at_unix_ms"

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }
}
