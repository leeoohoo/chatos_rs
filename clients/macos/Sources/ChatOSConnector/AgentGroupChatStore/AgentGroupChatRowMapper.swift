import ChatOSCore
import Foundation
import SQLite3

enum AgentGroupChatRowMapper {
    static func agent(_ statement: OpaquePointer) throws -> LocalAgentProfile {
        guard let status = LocalAgentProfileStatus(rawValue: string(statement, 15)) else {
            throw AgentGroupChatError.storage("invalid agent status")
        }
        let profile = LocalAgentProfile(
            id: string(statement, 1),
            ownerUserID: string(statement, 0),
            draft: .init(
                name: string(statement, 2),
                description: string(statement, 3),
                rolePrompt: string(statement, 4),
                modelConfigID: string(statement, 5),
                thinkingLevel: optionalString(statement, 6),
                professionKey: string(statement, 7),
                defaultPluginIDs: try decodeStrings(string(statement, 8)),
                defaultSkillIDs: try decodeStrings(string(statement, 9)),
                heartbeatEnabled: sqlite3_column_int64(statement, 10) != 0,
                heartbeatIntervalSeconds: Int(sqlite3_column_int64(statement, 11)),
                heartbeatPrompt: string(statement, 12)
            ),
            status: status,
            createdAtUnixMs: sqlite3_column_int64(statement, 16),
            updatedAtUnixMs: sqlite3_column_int64(statement, 17),
            lastHeartbeatAtUnixMs: optionalInt64(statement, 13),
            nextHeartbeatAtUnixMs: optionalInt64(statement, 14)
        )
        try profile.validate()
        return profile
    }

    static func room(_ statement: OpaquePointer) throws -> ProjectAgentRoom {
        guard let status = ProjectAgentRoomStatus(rawValue: string(statement, 6)),
              let conversationKind = LocalAgentConversationKind(
                rawValue: string(statement, 9)
              ) else {
            throw AgentGroupChatError.storage("invalid room status")
        }
        let room = ProjectAgentRoom(
            id: string(statement, 1),
            ownerUserID: string(statement, 0),
            projectID: string(statement, 2),
            draft: .init(name: string(statement, 3), goal: string(statement, 4)),
            defaultAgentID: optionalString(statement, 5),
            projectManagerAgentID: optionalString(statement, 11),
            conversationKind: conversationKind,
            directKey: optionalString(statement, 10),
            status: status,
            createdAtUnixMs: sqlite3_column_int64(statement, 7),
            updatedAtUnixMs: sqlite3_column_int64(statement, 8)
        )
        try room.validate()
        return room
    }

    static func member(_ statement: OpaquePointer) throws -> ProjectAgentRoomMember {
        guard let status = ProjectAgentRoomMemberStatus(rawValue: string(statement, 6)) else {
            throw AgentGroupChatError.storage("invalid member status")
        }
        let member = ProjectAgentRoomMember(
            ownerUserID: string(statement, 0),
            roomID: string(statement, 1),
            agentID: string(statement, 2),
            draft: .init(
                role: string(statement, 3),
                responsibility: string(statement, 4),
                pluginAllowlist: try decodeStrings(string(statement, 5))
            ),
            status: status,
            joinedAtUnixMs: sqlite3_column_int64(statement, 7)
        )
        try member.validate()
        return member
    }

    private static func decodeStrings(_ value: String) throws -> [String] {
        do {
            return try JSONDecoder().decode([String].self, from: Data(value.utf8))
        } catch {
            throw AgentGroupChatError.storage("invalid string list")
        }
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

    private static func optionalInt64(_ statement: OpaquePointer, _ index: Int32) -> Int64? {
        sqlite3_column_type(statement, index) == SQLITE_NULL
            ? nil
            : sqlite3_column_int64(statement, index)
    }
}
