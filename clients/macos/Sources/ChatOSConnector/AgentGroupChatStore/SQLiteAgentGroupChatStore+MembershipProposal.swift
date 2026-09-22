import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func createMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentMembershipProposalDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentMembershipProposal {
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
                  let proposer = try readAgent(
                    ownerUserID: ownerUserID,
                    agentID: proposerAgentID
                  ), proposer.status == .active,
                  LocalAgentPermission.canManageStaff(proposer.draft.defaultSkillIDs),
                  let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: sourceDeliveryID
                  ), delivery.roomID == sourceRoomID,
                     delivery.targetAgentID == proposerAgentID,
                     delivery.status == .running else {
                throw AgentGroupChatError.permissionDenied
            }
            if let existing = try readMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            guard let targetRoom = try readRoom(
                ownerUserID: ownerUserID,
                roomID: draft.targetTeamRoomID
            ), targetRoom.status == .active,
               targetRoom.conversationKind == .projectTeam,
               try readAgent(
                ownerUserID: ownerUserID,
                agentID: draft.targetAgentID
               )?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            guard try readMember(
                ownerUserID: ownerUserID,
                roomID: draft.targetTeamRoomID,
                agentID: draft.targetAgentID
            )?.status != .active else { throw AgentGroupChatError.conflict }

            let proposal = LocalAgentMembershipProposal(
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
                INSERT INTO local_agent_membership_proposals (
                    owner_user_id, id, source_room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?, NULL)
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

    public func listMembershipProposals(
        ownerUserID: String,
        sourceRoomID: String,
        status: LocalAgentMembershipProposalStatus? = nil
    ) throws -> [LocalAgentMembershipProposal] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: sourceRoomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentProposalRepository.listMembershipProposals(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func approveMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentMembershipProposalApproval {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let proposal = try readMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
                proposalID: proposalID
            ), proposal.status == .pending,
               let targetRoom = try readRoom(
                ownerUserID: ownerUserID,
                roomID: proposal.draft.targetTeamRoomID
               ), targetRoom.status == .active,
                  targetRoom.conversationKind == .projectTeam,
               let targetAgent = try readAgent(
                ownerUserID: ownerUserID,
                agentID: proposal.draft.targetAgentID
               ), targetAgent.status == .active else {
                throw AgentGroupChatError.conflict
            }
            let memberDraft = ProjectAgentRoomMemberDraft(
                role: proposal.draft.role,
                responsibility: proposal.draft.responsibility
            )
            let member: ProjectAgentRoomMember
            if let existing = try readMember(
                ownerUserID: ownerUserID,
                roomID: targetRoom.id,
                agentID: targetAgent.id
            ) {
                guard existing.status == .removed else { throw AgentGroupChatError.conflict }
                try execute(
                    """
                    UPDATE project_agent_room_members
                    SET role = ?, responsibility = ?, plugin_allowlist_json = '[]',
                        status = 'active', joined_at_unix_ms = ?
                    WHERE owner_user_id = ? AND room_id = ? AND agent_id = ?
                      AND status = 'removed'
                    """,
                    [
                        .text(memberDraft.role), .text(memberDraft.responsibility),
                        .integer(nowUnixMs), .text(ownerUserID), .text(targetRoom.id),
                        .text(targetAgent.id),
                    ]
                )
                guard sqlite3_changes(database) == 1,
                      let restored = try readMember(
                        ownerUserID: ownerUserID,
                        roomID: targetRoom.id,
                        agentID: targetAgent.id
                      ) else { throw AgentGroupChatError.conflict }
                member = restored
            } else {
                let created = ProjectAgentRoomMember(
                    ownerUserID: ownerUserID,
                    roomID: targetRoom.id,
                    agentID: targetAgent.id,
                    draft: memberDraft,
                    joinedAtUnixMs: nowUnixMs
                )
                try created.validate()
                try execute(
                    """
                    INSERT INTO project_agent_room_members (
                        owner_user_id, room_id, agent_id, role, responsibility,
                        plugin_allowlist_json, status, joined_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, '[]', 'active', ?)
                    """,
                    [
                        .text(ownerUserID), .text(targetRoom.id), .text(targetAgent.id),
                        .text(memberDraft.role), .text(memberDraft.responsibility),
                        .integer(nowUnixMs),
                    ]
                )
                member = created
            }
            let shouldAssignManager = targetRoom.projectManagerAgentID == nil
                && targetAgent.draft.professionKey == "project_manager"
            try execute(
                """
                UPDATE project_agent_rooms
                SET default_agent_id = COALESCE(default_agent_id, ?),
                    project_manager_agent_id = CASE
                        WHEN project_manager_agent_id IS NULL AND ? = 1 THEN ?
                        ELSE project_manager_agent_id
                    END,
                    updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [
                    .text(targetAgent.id), .integer(shouldAssignManager ? 1 : 0),
                    .text(targetAgent.id), .integer(nowUnixMs), .text(ownerUserID),
                    .text(targetRoom.id),
                ]
            )
            guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            try execute(
                """
                UPDATE local_agent_membership_proposals
                SET status = 'approved', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND source_room_id = ?
                  AND status = 'pending'
                """,
                [
                    .integer(nowUnixMs), .text(ownerUserID), .text(proposalID),
                    .text(sourceRoomID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let approved = try readMembershipProposal(
                    ownerUserID: ownerUserID,
                    sourceRoomID: sourceRoomID,
                    proposalID: proposalID
                  ), let updatedRoom = try readRoom(
                    ownerUserID: ownerUserID,
                    roomID: targetRoom.id
                  ) else { throw AgentGroupChatError.conflict }
            if shouldAssignManager {
                try enqueueTeamAssetMaintenanceNotification(
                    ownerUserID: ownerUserID,
                    roomID: targetRoom.id,
                    projectManagerAgentID: targetAgent.id,
                    nowUnixMs: nowUnixMs
                )
            }
            try enqueueProposalResolutionNotification(
                ownerUserID: ownerUserID,
                roomID: sourceRoomID,
                proposerAgentID: approved.proposerAgentID,
                proposalID: approved.id,
                proposalLabel: "现有 Agent 加入团队提案",
                approved: true,
                nowUnixMs: nowUnixMs
            )
            return .init(proposal: approved, member: member, room: updatedRoom)
        }
    }

    public func rejectMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentMembershipProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_agent_membership_proposals
                SET status = 'rejected', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND source_room_id = ?
                  AND status = 'pending'
                """,
                [
                    .integer(nowUnixMs), .text(ownerUserID), .text(proposalID),
                    .text(sourceRoomID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let rejected = try readMembershipProposal(
                    ownerUserID: ownerUserID,
                    sourceRoomID: sourceRoomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            try enqueueProposalResolutionNotification(
                ownerUserID: ownerUserID,
                roomID: sourceRoomID,
                proposerAgentID: rejected.proposerAgentID,
                proposalID: rejected.id,
                proposalLabel: "现有 Agent 加入团队提案",
                approved: false,
                nowUnixMs: nowUnixMs
            )
            return rejected
        }
    }

}
