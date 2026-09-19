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

    static func todo(_ statement: OpaquePointer) throws -> LocalAgentTodo {
        guard let status = LocalAgentTodoStatus(rawValue: string(statement, 11)) else {
            throw AgentGroupChatError.storage("invalid Agent todo status")
        }
        let executionPlan: LocalAgentTodoExecutionPlan
        let executionContract: LocalAgentTodoExecutionContract
        do {
            executionPlan = try JSONDecoder().decode(
                LocalAgentTodoExecutionPlan.self,
                from: Data(string(statement, 16).utf8)
            )
            executionContract = try JSONDecoder().decode(
                LocalAgentTodoExecutionContract.self,
                from: Data(string(statement, 17).utf8)
            ).normalized(
                title: string(statement, 6),
                detail: string(statement, 7)
            )
        } catch {
            throw AgentGroupChatError.storage("invalid Agent Todo execution contract")
        }
        guard let teamRoomID = optionalString(statement, 3) else {
            throw AgentGroupChatError.storage("Agent Todo is missing its team binding")
        }
        let todo = LocalAgentTodo(
            id: string(statement, 1),
            ownerUserID: string(statement, 0),
            agentID: string(statement, 2),
            teamRoomID: teamRoomID,
            sourceRoomID: optionalString(statement, 4),
            sourceMessageID: optionalString(statement, 5),
            title: string(statement, 6),
            detail: string(statement, 7),
            priority: Int(sqlite3_column_int64(statement, 8)),
            sortOrder: sqlite3_column_int64(statement, 9),
            status: status,
            blockedReason: string(statement, 12),
            result: string(statement, 13),
            executionContract: executionContract,
            executionPlan: executionPlan,
            createdAtUnixMs: sqlite3_column_int64(statement, 14),
            updatedAtUnixMs: sqlite3_column_int64(statement, 15)
        )
        try todo.validate()
        return todo
    }

    static func teamAsset(_ statement: OpaquePointer) throws -> LocalAgentTeamAsset {
        guard let category = LocalAgentTeamAssetCategory(rawValue: string(statement, 3)),
              let status = LocalAgentTeamAssetStatus(rawValue: string(statement, 7)) else {
            throw AgentGroupChatError.storage("invalid team asset")
        }
        let asset = LocalAgentTeamAsset(
            id: string(statement, 1),
            ownerUserID: string(statement, 0),
            teamRoomID: string(statement, 2),
            category: category,
            title: string(statement, 4),
            markdown: string(statement, 5),
            revision: Int(sqlite3_column_int64(statement, 6)),
            status: status,
            createdByAgentID: optionalString(statement, 8),
            updatedByAgentID: optionalString(statement, 9),
            createdAtUnixMs: sqlite3_column_int64(statement, 10),
            updatedAtUnixMs: sqlite3_column_int64(statement, 11)
        )
        try asset.validate()
        return asset
    }

    static func todoTeamAssetSnapshot(
        _ statement: OpaquePointer
    ) throws -> LocalAgentTodoTeamAssetSnapshot {
        guard let category = LocalAgentTeamAssetCategory(rawValue: string(statement, 3)) else {
            throw AgentGroupChatError.storage("invalid Todo team asset snapshot")
        }
        return .init(
            todoID: string(statement, 0),
            assetID: string(statement, 1),
            teamRoomID: string(statement, 2),
            category: category,
            title: string(statement, 4),
            markdown: string(statement, 5),
            revision: Int(sqlite3_column_int64(statement, 6)),
            capturedAtUnixMs: sqlite3_column_int64(statement, 7)
        )
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
