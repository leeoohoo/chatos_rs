import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func createAgent(
        ownerUserID: String,
        draft: LocalAgentProfileDraft
    ) throws -> LocalAgentProfile {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try draft.validate()
        let now = Self.now()
        let nextHeartbeatAtUnixMs = draft.heartbeatEnabled
            ? now + Int64(draft.heartbeatIntervalSeconds) * 1_000
            : nil
        let record = LocalAgentProfile(
            id: UUID().uuidString.lowercased(),
            ownerUserID: ownerUserID,
            draft: draft,
            createdAtUnixMs: now,
            updatedAtUnixMs: now,
            nextHeartbeatAtUnixMs: nextHeartbeatAtUnixMs
        )
        try record.validate()
        try execute(
            """
            INSERT INTO local_agent_profiles (
                owner_user_id, id, name, description, role_prompt, model_config_id,
                thinking_level, profession_key, default_plugin_ids_json, default_skill_ids_json,
                heartbeat_enabled, heartbeat_interval_seconds, heartbeat_prompt,
                last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms,
                status, created_at_unix_ms, updated_at_unix_ms, avatar_data
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)
            """,
            [
                .text(ownerUserID), .text(record.id), .text(draft.name),
                .text(draft.description), .text(draft.rolePrompt), .text(draft.modelConfigID),
                draft.thinkingLevel.map(Value.text) ?? .null,
                .text(draft.professionKey),
                .text(try encodeStrings(draft.defaultPluginIDs)),
                .text(try encodeStrings(draft.defaultSkillIDs)),
                .integer(draft.heartbeatEnabled ? 1 : 0),
                .integer(Int64(draft.heartbeatIntervalSeconds)), .text(draft.heartbeatPrompt),
                nextHeartbeatAtUnixMs.map(Value.integer) ?? .null, .text(record.status.rawValue),
                .integer(now), .integer(now),
                .optionalBlob(draft.avatarData),
            ]
        )
        return record
    }

    public func listAgents(
        ownerUserID: String,
        includeArchived: Bool = false
    ) throws -> [LocalAgentProfile] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentProfileRepository.list(
            database,
            ownerUserID: ownerUserID,
            includeArchived: includeArchived,
            preparedStatement: recordPreparedStatement
        )
    }

    public func nextAgentHeartbeatDue(ownerUserID: String) throws -> Int64? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentProfileRepository.nextHeartbeatDue(
            database,
            ownerUserID: ownerUserID,
            preparedStatement: recordPreparedStatement
        )
    }

    /// Creates exactly one account-wide inbox wake-up for each due Agent. The wake-up is anchored
    /// to the Agent's Human direct conversation only because the existing durable run queue needs
    /// a room authority; Relay reads every unread conversation in that one run.
    public func enqueueDueAgentHeartbeats(
        ownerUserID: String,
        nowUnixMs: Int64,
        agentLimit: Int = 16
    ) throws -> [ProjectAgentDelivery] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        guard (1...128).contains(agentLimit) else {
            throw AgentGroupChatError.invalidField("agentLimit")
        }
        return try transaction {
            let dueAgents = try AgentProfileRepository.dueHeartbeatAgents(
                database,
                ownerUserID: ownerUserID,
                nowUnixMs: nowUnixMs,
                limit: agentLimit,
                preparedStatement: recordPreparedStatement
            )
            var messages: [ProjectAgentMessage] = []
            var deliveries: [ProjectAgentDelivery] = []
            var newRooms: [ProjectAgentRoom] = []
            var newMembers: [ProjectAgentRoomMember] = []
            for agent in dueAgents {
                if !agent.hasOutstandingHeartbeat {
                    let roomID: String
                    if let existingRoomID = agent.directRoomID {
                        roomID = existingRoomID
                    } else {
                        let createdRoomID = UUID().uuidString.lowercased()
                        let room = ProjectAgentRoom(
                            id: createdRoomID,
                            ownerUserID: ownerUserID,
                            projectID: "direct:\(createdRoomID)",
                            draft: .init(name: agent.name),
                            defaultAgentID: agent.id,
                            conversationKind: .humanAgentDirect,
                            directKey: "human:\(ownerUserID)|agent:\(agent.id)",
                            createdAtUnixMs: nowUnixMs,
                            updatedAtUnixMs: nowUnixMs
                        )
                        try room.validate()
                        let member = ProjectAgentRoomMember(
                            ownerUserID: ownerUserID,
                            roomID: room.id,
                            agentID: agent.id,
                            draft: .init(
                                role: agent.name,
                                responsibility: agent.description
                            ),
                            joinedAtUnixMs: nowUnixMs
                        )
                        try member.validate()
                        newRooms.append(room)
                        newMembers.append(member)
                        roomID = room.id
                    }
                    let messageID = UUID().uuidString.lowercased()
                    let deliveryID = UUID().uuidString.lowercased()
                    let content = agent.prompt.isEmpty
                        ? "主动巡检：读取全部未读消息，整理并继续处理自己的 TodoList。"
                        : agent.prompt
                    messages.append(.init(
                        id: messageID,
                        ownerUserID: ownerUserID,
                        roomID: roomID,
                        draft: .init(
                            senderKind: .system,
                            senderID: "system",
                            content: content,
                            causationID: "heartbeat",
                            rootMessageID: messageID
                        ),
                        rootMessageID: messageID,
                        createdAtUnixMs: nowUnixMs
                    ))
                    deliveries.append(ProjectAgentDelivery(
                        id: deliveryID,
                        ownerUserID: ownerUserID,
                        roomID: roomID,
                        messageID: messageID,
                        rootMessageID: messageID,
                        targetAgentID: agent.id,
                        triggerKind: .heartbeat,
                        status: .pending,
                        attempt: 0,
                        hopCount: 0,
                        deduplicationKey: "heartbeat:\(agent.id):\(agent.scheduledAtUnixMs)",
                        createdAtUnixMs: nowUnixMs
                    ))
                }
            }
            try AgentConversationRepository.insertRooms(
                database,
                rooms: newRooms,
                preparedStatement: recordPreparedStatement
            )
            try AgentConversationRepository.insertMembers(
                database,
                members: newMembers,
                preparedStatement: recordPreparedStatement
            )
            try AgentMessageRepository.insert(
                database,
                messages: messages,
                preparedStatement: recordPreparedStatement
            )
            try AgentDeliveryRepository.insert(
                database,
                deliveries: deliveries,
                preparedStatement: recordPreparedStatement
            )
            try AgentProfileRepository.updateHeartbeatSchedule(
                database,
                ownerUserID: ownerUserID,
                schedules: dueAgents.map {
                    ($0.id, nowUnixMs + $0.intervalSeconds * 1_000)
                },
                nowUnixMs: nowUnixMs,
                preparedStatement: recordPreparedStatement
            )
            return deliveries
        }
    }

    /// Queues at most one highest-priority sourced Todo per Agent. The ordinary per-Agent delivery
    /// lock keeps Todo execution serial, while the source room restores the correct project tools.

    public func updateAgentProfile(
        ownerUserID: String,
        agentID: String,
        draft: LocalAgentProfileDraft
    ) throws -> LocalAgentProfile {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try draft.validate()
        return try transaction {
            guard let existing = try readAgent(ownerUserID: ownerUserID, agentID: agentID),
                  existing.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let now = max(Self.now(), existing.updatedAtUnixMs)
            let nextHeartbeatAtUnixMs: Int64?
            if draft.heartbeatEnabled {
                if !existing.draft.heartbeatEnabled
                    || existing.draft.heartbeatIntervalSeconds != draft.heartbeatIntervalSeconds {
                    nextHeartbeatAtUnixMs = now + Int64(draft.heartbeatIntervalSeconds) * 1_000
                } else {
                    nextHeartbeatAtUnixMs = existing.nextHeartbeatAtUnixMs
                        ?? now + Int64(draft.heartbeatIntervalSeconds) * 1_000
                }
            } else {
                nextHeartbeatAtUnixMs = nil
            }
            try execute(
                """
                UPDATE local_agent_profiles
                SET name = ?, description = ?, role_prompt = ?, model_config_id = ?,
                    thinking_level = ?, profession_key = ?, default_plugin_ids_json = ?,
                    default_skill_ids_json = ?, heartbeat_enabled = ?,
                    heartbeat_interval_seconds = ?, heartbeat_prompt = ?,
                    next_heartbeat_at_unix_ms = ?, avatar_data = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [
                    .text(draft.name), .text(draft.description), .text(draft.rolePrompt),
                    .text(draft.modelConfigID), draft.thinkingLevel.map(Value.text) ?? .null,
                    .text(draft.professionKey), .text(try encodeStrings(draft.defaultPluginIDs)),
                    .text(try encodeStrings(draft.defaultSkillIDs)),
                    .integer(draft.heartbeatEnabled ? 1 : 0),
                    .integer(Int64(draft.heartbeatIntervalSeconds)), .text(draft.heartbeatPrompt),
                    nextHeartbeatAtUnixMs.map(Value.integer) ?? .null,
                    .optionalBlob(draft.avatarData), .integer(now),
                    .text(ownerUserID), .text(agentID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readAgent(ownerUserID: ownerUserID, agentID: agentID) else {
                throw AgentGroupChatError.conflict
            }
            return updated
        }
    }

    /// Safely removes an Agent from active use while preserving its profile, messages, Runs and
    /// audit history. All dependent active state is closed in the same transaction so no scheduler
    /// can continue dispatching work to an archived profile.
    public func archiveAgent(
        ownerUserID: String,
        agentID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentProfile {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let existing = try readAgent(ownerUserID: ownerUserID, agentID: agentID),
                  existing.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let archivedAt = max(nowUnixMs, existing.updatedAtUnixMs)
            let reason = "Agent 已由用户删除。"

            _ = try stopOutstandingDeliveriesForAgentInTransaction(
                ownerUserID: ownerUserID,
                agentID: agentID,
                reason: reason,
                nowUnixMs: archivedAt
            )
            try execute(
                """
                UPDATE local_agent_todos
                SET status = 'cancelled', blocked_reason = '', result = ?,
                    updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND agent_id = ?
                  AND status IN ('pending', 'in_progress', 'blocked')
                """,
                [.text(reason), .integer(archivedAt), .text(ownerUserID), .text(agentID)]
            )

            // Direct chats are hidden from active navigation but remain intact for local history.
            try execute(
                """
                UPDATE project_agent_rooms
                SET status = 'archived', updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND status = 'active'
                  AND conversation_kind != 'project_team'
                  AND id IN (
                    SELECT room_id FROM project_agent_room_members
                    WHERE owner_user_id = ? AND agent_id = ? AND status = 'active'
                  )
                """,
                [
                    .integer(archivedAt), .text(ownerUserID),
                    .text(ownerUserID), .text(agentID),
                ]
            )
            try execute(
                """
                UPDATE project_agent_room_members
                SET status = 'removed'
                WHERE owner_user_id = ? AND agent_id = ? AND status = 'active'
                  AND room_id IN (
                    SELECT id FROM project_agent_rooms
                    WHERE owner_user_id = ? AND conversation_kind = 'project_team'
                  )
                """,
                [.text(ownerUserID), .text(agentID), .text(ownerUserID)]
            )
            // Repoint defaults after membership removal; an empty team legitimately has no default.
            try execute(
                """
                UPDATE project_agent_rooms
                SET default_agent_id = (
                        SELECT member.agent_id
                        FROM project_agent_room_members member
                        JOIN local_agent_profiles profile
                          ON profile.owner_user_id = member.owner_user_id
                         AND profile.id = member.agent_id
                        WHERE member.owner_user_id = project_agent_rooms.owner_user_id
                          AND member.room_id = project_agent_rooms.id
                          AND member.status = 'active' AND profile.status = 'active'
                        ORDER BY member.joined_at_unix_ms, member.agent_id LIMIT 1
                    ),
                    updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND status = 'active'
                  AND conversation_kind = 'project_team' AND default_agent_id = ?
                """,
                [.integer(archivedAt), .text(ownerUserID), .text(agentID)]
            )
            // Project-manager authority is never transferred to an arbitrary profession. Prefer
            // another active project manager when one exists; otherwise leave the team unmanaged.
            try execute(
                """
                UPDATE project_agent_rooms
                SET project_manager_agent_id = (
                        SELECT member.agent_id
                        FROM project_agent_room_members member
                        JOIN local_agent_profiles profile
                          ON profile.owner_user_id = member.owner_user_id
                         AND profile.id = member.agent_id
                        WHERE member.owner_user_id = project_agent_rooms.owner_user_id
                          AND member.room_id = project_agent_rooms.id
                          AND member.status = 'active' AND profile.status = 'active'
                          AND profile.profession_key = 'project_manager'
                        ORDER BY member.joined_at_unix_ms, member.agent_id LIMIT 1
                    ),
                    updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND status = 'active'
                  AND conversation_kind = 'project_team' AND project_manager_agent_id = ?
                """,
                [.integer(archivedAt), .text(ownerUserID), .text(agentID)]
            )
            try execute(
                """
                UPDATE local_agent_profiles
                SET status = 'archived', heartbeat_enabled = 0,
                    next_heartbeat_at_unix_ms = NULL, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [.integer(archivedAt), .text(ownerUserID), .text(agentID)]
            )
            guard sqlite3_changes(database) == 1,
                  let archived = try readAgent(ownerUserID: ownerUserID, agentID: agentID) else {
                throw AgentGroupChatError.conflict
            }
            return archived
        }
    }

    public func updateAgentMembership(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        profileDraft: LocalAgentProfileDraft,
        memberDraft: ProjectAgentRoomMemberDraft
    ) throws -> LocalAgentMembershipUpdateResult {
        try validateOwnerRoomAgent(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)
        try profileDraft.validate()
        try memberDraft.validate()
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active,
                  let profile = try readAgent(ownerUserID: ownerUserID, agentID: agentID),
                  profile.status == .active,
                  let member = try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: agentID
                  ), member.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let now = max(Self.now(), profile.updatedAtUnixMs)
            try execute(
                """
                UPDATE local_agent_profiles
                SET name = ?, description = ?, role_prompt = ?, model_config_id = ?,
                    thinking_level = ?, profession_key = ?, default_plugin_ids_json = ?,
                    default_skill_ids_json = ?, avatar_data = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [
                    .text(profileDraft.name), .text(profileDraft.description),
                    .text(profileDraft.rolePrompt), .text(profileDraft.modelConfigID),
                    profileDraft.thinkingLevel.map(Value.text) ?? .null,
                    .text(profileDraft.professionKey),
                    .text(try encodeStrings(profileDraft.defaultPluginIDs)),
                    .text(try encodeStrings(profileDraft.defaultSkillIDs)),
                    .optionalBlob(profileDraft.avatarData), .integer(now),
                    .text(ownerUserID), .text(agentID),
                ]
            )
            guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            try execute(
                """
                UPDATE project_agent_room_members
                SET role = ?, responsibility = ?, plugin_allowlist_json = ?
                WHERE owner_user_id = ? AND room_id = ? AND agent_id = ? AND status = 'active'
                """,
                [
                    .text(memberDraft.role), .text(memberDraft.responsibility),
                    .text(try encodeStrings(memberDraft.pluginAllowlist)), .text(ownerUserID),
                    .text(roomID), .text(agentID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let updatedProfile = try readAgent(ownerUserID: ownerUserID, agentID: agentID),
                  let updatedMember = try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: agentID
                  ) else {
                throw AgentGroupChatError.conflict
            }
            return .init(profile: updatedProfile, member: updatedMember)
        }
    }

}
