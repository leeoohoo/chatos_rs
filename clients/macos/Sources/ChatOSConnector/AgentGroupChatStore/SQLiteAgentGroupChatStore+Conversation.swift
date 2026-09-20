import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func createRoom(
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft
    ) throws -> ProjectAgentRoom {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try draft.validate()
        return try transaction {
            guard try readActiveRoom(ownerUserID: ownerUserID, projectID: projectID) == nil else {
                throw AgentGroupChatError.conflict
            }
            let now = Self.now()
            let room = ProjectAgentRoom(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: draft,
                createdAtUnixMs: now,
                updatedAtUnixMs: now
            )
            try room.validate()
            try execute(
                """
                INSERT INTO project_agent_rooms (
                    owner_user_id, id, project_id, name, goal, default_agent_id, status,
                    created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, NULL, 'active', ?, ?)
                """,
                [
                    .text(ownerUserID), .text(room.id), .text(projectID), .text(draft.name),
                    .text(draft.goal), .integer(now), .integer(now),
                ]
            )
            return room
        }
    }

    public func createManagedRoom(
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft,
        projectManagerAgentID: String
    ) throws -> ProjectAgentRoom {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try AgentGroupChatValidation.identifier(
            projectManagerAgentID,
            field: "projectManagerAgentID"
        )
        try draft.validate()
        return try transaction {
            guard try readActiveRoom(ownerUserID: ownerUserID, projectID: projectID) == nil else {
                throw AgentGroupChatError.conflict
            }
            guard let manager = try readAgent(
                ownerUserID: ownerUserID,
                agentID: projectManagerAgentID
            ), manager.status == .active,
            manager.draft.professionKey == "project_manager" else {
                throw AgentGroupChatError.invalidField("projectManagerProfession")
            }
            let timestamp = Self.now()
            let room = ProjectAgentRoom(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: draft,
                defaultAgentID: projectManagerAgentID,
                projectManagerAgentID: projectManagerAgentID,
                createdAtUnixMs: timestamp,
                updatedAtUnixMs: timestamp
            )
            try room.validate()
            try insertConversation(room)
            try insertDirectMember(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agent: manager,
                nowUnixMs: timestamp
            )
            return room
        }
    }

    public func openHumanAgentDirect(
        ownerUserID: String,
        agentID: String
    ) throws -> ProjectAgentRoom {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        let directKey = "human:\(ownerUserID)|agent:\(agentID)"
        try AgentGroupChatValidation.identifier(directKey, field: "directKey")
        return try transaction {
            if let existing = try readDirectRoom(ownerUserID: ownerUserID, directKey: directKey) {
                return existing
            }
            guard let agent = try readAgent(ownerUserID: ownerUserID, agentID: agentID),
                  agent.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let now = Self.now()
            let roomID = UUID().uuidString.lowercased()
            let room = ProjectAgentRoom(
                id: roomID,
                ownerUserID: ownerUserID,
                projectID: "direct:\(roomID)",
                draft: .init(name: agent.draft.name),
                defaultAgentID: agentID,
                conversationKind: .humanAgentDirect,
                directKey: directKey,
                createdAtUnixMs: now,
                updatedAtUnixMs: now
            )
            try room.validate()
            try insertConversation(room)
            try insertDirectMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agent: agent,
                nowUnixMs: now
            )
            return room
        }
    }

    public func openAgentDirect(
        ownerUserID: String,
        initiatingAgentID: String,
        targetAgentID: String
    ) throws -> ProjectAgentRoom {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(initiatingAgentID, field: "initiatingAgentID")
        try AgentGroupChatValidation.identifier(targetAgentID, field: "targetAgentID")
        guard initiatingAgentID != targetAgentID else {
            throw AgentGroupChatError.invalidField("targetAgentID")
        }
        let pair = [initiatingAgentID, targetAgentID].sorted()
        let directKey = "agent:\(pair[0])|agent:\(pair[1])"
        try AgentGroupChatValidation.identifier(directKey, field: "directKey")
        return try transaction {
            if let existing = try readDirectRoom(ownerUserID: ownerUserID, directKey: directKey) {
                return existing
            }
            guard let first = try readAgent(ownerUserID: ownerUserID, agentID: pair[0]),
                  first.status == .active,
                  let second = try readAgent(ownerUserID: ownerUserID, agentID: pair[1]),
                  second.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let now = Self.now()
            let roomID = UUID().uuidString.lowercased()
            let room = ProjectAgentRoom(
                id: roomID,
                ownerUserID: ownerUserID,
                projectID: "direct:\(roomID)",
                draft: .init(name: "\(first.draft.name) · \(second.draft.name)"),
                conversationKind: .agentAgentDirect,
                directKey: directKey,
                createdAtUnixMs: now,
                updatedAtUnixMs: now
            )
            try room.validate()
            try insertConversation(room)
            try insertDirectMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agent: first,
                nowUnixMs: now
            )
            try insertDirectMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agent: second,
                nowUnixMs: now
            )
            return room
        }
    }

    public func room(ownerUserID: String, roomID: String) throws -> ProjectAgentRoom? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        return try readRoom(ownerUserID: ownerUserID, roomID: roomID)
    }

    public func activeRoom(ownerUserID: String, projectID: String) throws -> ProjectAgentRoom? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        return try readActiveRoom(ownerUserID: ownerUserID, projectID: projectID)
    }

    public func listRooms(
        ownerUserID: String,
        includeArchived: Bool = false
    ) throws -> [ProjectAgentRoom] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentConversationRepository.listProjectRooms(
            database,
            ownerUserID: ownerUserID,
            includeArchived: includeArchived,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listDirectConversations(
        ownerUserID: String,
        includeArchived: Bool = false
    ) throws -> [ProjectAgentRoom] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentConversationRepository.listDirectRooms(
            database,
            ownerUserID: ownerUserID,
            includeArchived: includeArchived,
            preparedStatement: recordPreparedStatement
        )
    }

    public func addMember(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        draft: ProjectAgentRoomMemberDraft
    ) throws -> ProjectAgentRoomMember {
        try validateOwnerRoomAgent(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)
        try draft.validate()
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: roomID)?.status == .active,
                  try readAgent(ownerUserID: ownerUserID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            if let existing = try readMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID
            ) {
                guard existing.status == .removed else { throw AgentGroupChatError.conflict }
                let joinedAtUnixMs = Self.now()
                try execute(
                    """
                    UPDATE project_agent_room_members
                    SET role = ?, responsibility = ?, plugin_allowlist_json = ?,
                        status = 'active', joined_at_unix_ms = ?
                    WHERE owner_user_id = ? AND room_id = ? AND agent_id = ? AND status = 'removed'
                    """,
                    [
                        .text(draft.role), .text(draft.responsibility),
                        .text(try encodeStrings(draft.pluginAllowlist)), .integer(joinedAtUnixMs),
                        .text(ownerUserID), .text(roomID), .text(agentID),
                    ]
                )
                guard sqlite3_changes(database) == 1,
                      let restored = try readMember(
                        ownerUserID: ownerUserID,
                        roomID: roomID,
                        agentID: agentID
                      ) else { throw AgentGroupChatError.conflict }
                return restored
            }
            let member = ProjectAgentRoomMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID,
                draft: draft,
                joinedAtUnixMs: Self.now()
            )
            try member.validate()
            try execute(
                """
                INSERT INTO project_agent_room_members (
                    owner_user_id, room_id, agent_id, role, responsibility,
                    plugin_allowlist_json, status, joined_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, 'active', ?)
                """,
                [
                    .text(ownerUserID), .text(roomID), .text(agentID), .text(draft.role),
                    .text(draft.responsibility), .text(try encodeStrings(draft.pluginAllowlist)),
                    .integer(member.joinedAtUnixMs),
                ]
            )
            return member
        }
    }

    public func listMembers(
        ownerUserID: String,
        roomID: String
    ) throws -> [ProjectAgentRoomMember] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentConversationRepository.listMembers(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func setDefaultAgent(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) throws -> ProjectAgentRoom {
        try validateOwnerRoomAgent(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active else {
                throw AgentGroupChatError.notFound
            }
            guard try readMember(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notMember
            }
            let now = max(Self.now(), room.updatedAtUnixMs)
            try execute(
                """
                UPDATE project_agent_rooms SET default_agent_id = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [.text(agentID), .integer(now), .text(ownerUserID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readRoom(ownerUserID: ownerUserID, roomID: roomID) else {
                throw AgentGroupChatError.conflict
            }
            return updated
        }
    }

    public func setProjectManager(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) throws -> ProjectAgentRoom {
        try validateOwnerRoomAgent(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active, room.conversationKind == .projectTeam else {
                throw AgentGroupChatError.notFound
            }
            guard try readMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID
            )?.status == .active else { throw AgentGroupChatError.notMember }
            guard try readAgent(
                ownerUserID: ownerUserID,
                agentID: agentID
            )?.draft.professionKey == "project_manager" else {
                throw AgentGroupChatError.invalidField("projectManagerProfession")
            }
            let now = max(Self.now(), room.updatedAtUnixMs)
            try execute(
                """
                UPDATE project_agent_rooms
                SET project_manager_agent_id = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [.text(agentID), .integer(now), .text(ownerUserID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readRoom(ownerUserID: ownerUserID, roomID: roomID) else {
                throw AgentGroupChatError.conflict
            }
            return updated
        }
    }

}
