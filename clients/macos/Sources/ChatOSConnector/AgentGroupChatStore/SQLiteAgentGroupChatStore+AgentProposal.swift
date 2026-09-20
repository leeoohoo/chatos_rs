import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func createAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentCreationProposal {
        try validateOwnerRoomAgent(
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: proposerAgentID
        )
        try AgentGroupChatValidation.identifier(sourceDeliveryID, field: "sourceDeliveryID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try draft.validate()
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: roomID)?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: proposerAgentID
                  )?.status == .active,
                  let profile = try readAgent(
                    ownerUserID: ownerUserID,
                    agentID: proposerAgentID
                  ), LocalAgentPermission.canManageStaff(profile.draft.defaultSkillIDs),
                  let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: sourceDeliveryID
                  ), delivery.roomID == roomID,
                     delivery.targetAgentID == proposerAgentID,
                     delivery.status == .running else {
                throw AgentGroupChatError.permissionDenied
            }
            if let existing = try readProposal(
                ownerUserID: ownerUserID,
                roomID: roomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            let proposal = LocalAgentCreationProposal(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                roomID: roomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey,
                draft: draft,
                createdAtUnixMs: nowUnixMs
            )
            try proposal.validate()
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let draftJSON = String(decoding: try encoder.encode(draft), as: UTF8.self)
            try execute(
                """
                INSERT INTO local_agent_creation_proposals (
                    owner_user_id, id, room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_agent_id,
                    created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?, NULL)
                """,
                [
                    .text(ownerUserID), .text(proposal.id), .text(roomID),
                    .text(proposerAgentID), .text(sourceDeliveryID), .text(requestKey),
                    .text(draftJSON), .integer(nowUnixMs),
                ]
            )
            return proposal
        }
    }

    public func listAgentProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalAgentCreationProposalStatus? = nil
    ) throws -> [LocalAgentCreationProposal] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentProposalRepository.listCreationProposals(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func approveAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentProposalApproval {
        try approveAgentProposal(
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposalID,
            nowUnixMs: nowUnixMs,
            draftOverride: nil
        )
    }

    public func approveAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64,
        resolvedDraft: LocalAgentDraft
    ) throws -> LocalAgentProposalApproval {
        try approveAgentProposal(
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposalID,
            nowUnixMs: nowUnixMs,
            draftOverride: resolvedDraft
        )
    }

    private func approveAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64,
        draftOverride: LocalAgentDraft?
    ) throws -> LocalAgentProposalApproval {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active,
                  let proposal = try readProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ), proposal.status == .pending else {
                throw AgentGroupChatError.conflict
            }
            let approvedDraft = draftOverride ?? proposal.draft
            if draftOverride != nil, approvedDraft != proposal.draft {
                let storedModelConfigID = proposal.draft.modelConfigID
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                let expectedDraft = LocalAgentDraft(
                    name: proposal.draft.name,
                    role: proposal.draft.role,
                    responsibility: proposal.draft.responsibility,
                    rolePrompt: proposal.draft.rolePrompt,
                    modelConfigID: approvedDraft.modelConfigID,
                    thinkingLevel: approvedDraft.thinkingLevel,
                    professionKey: proposal.draft.professionKey,
                    rationale: proposal.draft.rationale
                )
                guard LocalAgentBuilderService.usesProposerModel(storedModelConfigID),
                      approvedDraft == expectedDraft else {
                    throw AgentGroupChatError.conflict
                }
            }
            try approvedDraft.validate()
            let agent = LocalAgentProfile(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                draft: approvedDraft.profileDraft,
                createdAtUnixMs: nowUnixMs,
                updatedAtUnixMs: nowUnixMs
            )
            try agent.validate()
            try execute(
                """
                INSERT INTO local_agent_profiles (
                    owner_user_id, id, name, description, role_prompt, model_config_id,
                    thinking_level, profession_key, default_plugin_ids_json,
                    default_skill_ids_json, heartbeat_enabled, heartbeat_interval_seconds,
                    heartbeat_prompt, last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms,
                    status, created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 900, '', NULL, NULL, 'active', ?, ?)
                """,
                [
                    .text(ownerUserID), .text(agent.id), .text(agent.draft.name),
                    .text(agent.draft.description), .text(agent.draft.rolePrompt),
                    .text(agent.draft.modelConfigID),
                    agent.draft.thinkingLevel.map(Value.text) ?? .null,
                    .text(agent.draft.professionKey),
                    .text(try encodeStrings(agent.draft.defaultPluginIDs)),
                    .text(try encodeStrings(agent.draft.defaultSkillIDs)),
                    .integer(nowUnixMs), .integer(nowUnixMs),
                ]
            )
            var member: ProjectAgentRoomMember?
            if room.conversationKind == .projectTeam {
                let createdMember = ProjectAgentRoomMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: agent.id,
                    draft: approvedDraft.memberDraft,
                    joinedAtUnixMs: nowUnixMs
                )
                try createdMember.validate()
                try execute(
                    """
                    INSERT INTO project_agent_room_members (
                        owner_user_id, room_id, agent_id, role, responsibility,
                        plugin_allowlist_json, status, joined_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, ?, 'active', ?)
                    """,
                    [
                        .text(ownerUserID), .text(roomID), .text(agent.id),
                        .text(createdMember.draft.role), .text(createdMember.draft.responsibility),
                        .text(try encodeStrings(createdMember.draft.pluginAllowlist)), .integer(nowUnixMs),
                    ]
                )
                member = createdMember
                if room.defaultAgentID == nil {
                    try execute(
                        """
                        UPDATE project_agent_rooms SET default_agent_id = ?, updated_at_unix_ms = ?
                        WHERE owner_user_id = ? AND id = ? AND status = 'active'
                        """,
                        [.text(agent.id), .integer(nowUnixMs), .text(ownerUserID), .text(roomID)]
                    )
                }
            }
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let draftJSON = String(
                decoding: try encoder.encode(approvedDraft),
                as: UTF8.self
            )
            try execute(
                """
                UPDATE local_agent_creation_proposals
                SET draft_json = ?, status = 'approved', created_agent_id = ?, resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [
                    .text(draftJSON), .text(agent.id), .integer(nowUnixMs), .text(ownerUserID),
                    .text(proposalID), .text(roomID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let approved = try readProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return .init(proposal: approved, agent: agent, member: member)
        }
    }

    public func rejectAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentCreationProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_agent_creation_proposals
                SET status = 'rejected', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(proposalID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let rejected = try readProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return rejected
        }
    }

}
