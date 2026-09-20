import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func createTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentTeamCreationProposalDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamCreationProposal {
        try validateOwnerRoomAgent(
            ownerUserID: ownerUserID,
            roomID: sourceRoomID,
            agentID: proposerAgentID
        )
        try AgentGroupChatValidation.identifier(sourceDeliveryID, field: "sourceDeliveryID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try draft.validate()
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: sourceRoomID)?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: sourceRoomID,
                    agentID: proposerAgentID
                  )?.status == .active,
                  let profile = try readAgent(
                    ownerUserID: ownerUserID,
                    agentID: proposerAgentID
                  ), LocalAgentPermission.canAccessLocalProjects(profile.draft.defaultSkillIDs),
                  let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: sourceDeliveryID
                  ), delivery.roomID == sourceRoomID,
                     delivery.targetAgentID == proposerAgentID,
                     delivery.status == .running else {
                throw AgentGroupChatError.permissionDenied
            }
            if let existing = try readTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            let proposal = LocalAgentTeamCreationProposal(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
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
                INSERT INTO local_agent_team_creation_proposals (
                    owner_user_id, id, source_room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_room_id,
                    created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?, NULL)
                """,
                [
                    .text(ownerUserID), .text(proposal.id), .text(sourceRoomID),
                    .text(proposerAgentID), .text(sourceDeliveryID), .text(requestKey),
                    .text(draftJSON), .integer(nowUnixMs),
                ]
            )
            return proposal
        }
    }

    public func listTeamProposals(
        ownerUserID: String,
        sourceRoomID: String,
        status: LocalAgentTeamCreationProposalStatus? = nil
    ) throws -> [LocalAgentTeamCreationProposal] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: sourceRoomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentProposalRepository.listTeamProposals(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func approveTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        resolvedProjectID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamProposalApproval {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        try AgentGroupChatValidation.identifier(resolvedProjectID, field: "resolvedProjectID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let proposal = try readTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
                proposalID: proposalID
            ), proposal.status == .pending else {
                throw AgentGroupChatError.conflict
            }
            if let existingProjectID = proposal.draft.existingProjectID {
                guard existingProjectID == resolvedProjectID else {
                    throw AgentGroupChatError.permissionDenied
                }
            } else {
                guard proposal.draft.newProjectName != nil else {
                    throw AgentGroupChatError.conflict
                }
            }
            guard try readActiveRoom(
                ownerUserID: ownerUserID,
                projectID: resolvedProjectID
            ) == nil else { throw AgentGroupChatError.conflict }
            let room = ProjectAgentRoom(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                projectID: resolvedProjectID,
                draft: .init(
                    name: proposal.draft.teamName,
                    goal: proposal.draft.teamGoal
                ),
                createdAtUnixMs: nowUnixMs,
                updatedAtUnixMs: nowUnixMs
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
                    .text(ownerUserID), .text(room.id), .text(resolvedProjectID),
                    .text(room.draft.name), .text(room.draft.goal),
                    .integer(nowUnixMs), .integer(nowUnixMs),
                ]
            )
            try execute(
                """
                UPDATE local_agent_team_creation_proposals
                SET status = 'approved', created_room_id = ?, resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND source_room_id = ? AND status = 'pending'
                """,
                [
                    .text(room.id), .integer(nowUnixMs), .text(ownerUserID),
                    .text(proposalID), .text(sourceRoomID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let approved = try readTeamProposal(
                    ownerUserID: ownerUserID,
                    sourceRoomID: sourceRoomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return .init(proposal: approved, room: room)
        }
    }

    public func rejectTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamCreationProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_agent_team_creation_proposals
                SET status = 'rejected', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND source_room_id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(proposalID), .text(sourceRoomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let rejected = try readTeamProposal(
                    ownerUserID: ownerUserID,
                    sourceRoomID: sourceRoomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return rejected
        }
    }

}
