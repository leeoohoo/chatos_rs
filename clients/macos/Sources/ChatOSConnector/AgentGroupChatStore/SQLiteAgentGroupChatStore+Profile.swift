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
                status, created_at_unix_ms, updated_at_unix_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?)
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
            var deliveries: [ProjectAgentDelivery] = []
            for agent in dueAgents {
                let outstanding = try AgentDeliveryRepository.outstandingCount(
                    database,
                    ownerUserID: ownerUserID,
                    targetAgentID: agent.id,
                    triggerKind: .heartbeat,
                    preparedStatement: recordPreparedStatement
                )
                if outstanding == 0 {
                    let directKey = "human:\(ownerUserID)|agent:\(agent.id)"
                    let room: ProjectAgentRoom
                    if let existing = try readDirectRoom(
                        ownerUserID: ownerUserID,
                        directKey: directKey
                    ) {
                        room = existing
                    } else {
                        guard let profile = try readAgent(
                            ownerUserID: ownerUserID,
                            agentID: agent.id
                        ) else { throw AgentGroupChatError.notFound }
                        let roomID = UUID().uuidString.lowercased()
                        room = ProjectAgentRoom(
                            id: roomID,
                            ownerUserID: ownerUserID,
                            projectID: "direct:\(roomID)",
                            draft: .init(name: profile.draft.name),
                            defaultAgentID: profile.id,
                            conversationKind: .humanAgentDirect,
                            directKey: directKey,
                            createdAtUnixMs: nowUnixMs,
                            updatedAtUnixMs: nowUnixMs
                        )
                        try room.validate()
                        try insertConversation(room)
                        try insertDirectMember(
                            ownerUserID: ownerUserID,
                            roomID: room.id,
                            agent: profile,
                            nowUnixMs: nowUnixMs
                        )
                    }
                    let messageID = UUID().uuidString.lowercased()
                    let deliveryID = UUID().uuidString.lowercased()
                    let content = agent.prompt.isEmpty
                        ? "主动巡检：读取全部未读消息，整理并继续处理自己的 TodoList。"
                        : agent.prompt
                    try execute(
                        """
                        INSERT INTO project_agent_messages (
                            owner_user_id, id, room_id, sender_kind, sender_id, content,
                            reply_to_message_id, source_run_id, causation_id, root_message_id,
                            hop_count, created_at_unix_ms
                        ) VALUES (?, ?, ?, 'system', 'system', ?, NULL, NULL, 'heartbeat', ?, 0, ?)
                        """,
                        [
                            .text(ownerUserID), .text(messageID), .text(room.id), .text(content),
                            .text(messageID), .integer(nowUnixMs),
                        ]
                    )
                    try execute(
                        """
                        INSERT INTO project_agent_deliveries (
                            owner_user_id, id, room_id, message_id, root_message_id,
                            target_agent_id, trigger_kind, status, attempt, hop_count,
                            deduplication_key, response_message_id, last_error,
                            claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                        ) VALUES (?, ?, ?, ?, ?, ?, 'heartbeat', 'pending', 0, 0, ?, NULL, NULL, NULL, NULL, ?)
                        """,
                        [
                            .text(ownerUserID), .text(deliveryID), .text(room.id), .text(messageID),
                            .text(messageID), .text(agent.id),
                            .text("heartbeat:\(agent.id):\(agent.scheduledAtUnixMs)"),
                            .integer(nowUnixMs),
                        ]
                    )
                    guard let delivery = try readDelivery(
                        ownerUserID: ownerUserID,
                        deliveryID: deliveryID
                    ) else { throw AgentGroupChatError.storage("heartbeat delivery insert failed") }
                    deliveries.append(delivery)
                }
                let next = nowUnixMs + agent.intervalSeconds * 1_000
                try execute(
                    """
                    UPDATE local_agent_profiles
                    SET last_heartbeat_at_unix_ms = ?, next_heartbeat_at_unix_ms = ?
                    WHERE owner_user_id = ? AND id = ? AND status = 'active'
                      AND heartbeat_enabled = 1
                    """,
                    [.integer(nowUnixMs), .integer(next), .text(ownerUserID), .text(agent.id)]
                )
            }
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
                    next_heartbeat_at_unix_ms = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [
                    .text(draft.name), .text(draft.description), .text(draft.rolePrompt),
                    .text(draft.modelConfigID), draft.thinkingLevel.map(Value.text) ?? .null,
                    .text(draft.professionKey), .text(try encodeStrings(draft.defaultPluginIDs)),
                    .text(try encodeStrings(draft.defaultSkillIDs)),
                    .integer(draft.heartbeatEnabled ? 1 : 0),
                    .integer(Int64(draft.heartbeatIntervalSeconds)), .text(draft.heartbeatPrompt),
                    nextHeartbeatAtUnixMs.map(Value.integer) ?? .null, .integer(now),
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
                    default_skill_ids_json = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [
                    .text(profileDraft.name), .text(profileDraft.description),
                    .text(profileDraft.rolePrompt), .text(profileDraft.modelConfigID),
                    profileDraft.thinkingLevel.map(Value.text) ?? .null,
                    .text(profileDraft.professionKey),
                    .text(try encodeStrings(profileDraft.defaultPluginIDs)),
                    .text(try encodeStrings(profileDraft.defaultSkillIDs)), .integer(now),
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
