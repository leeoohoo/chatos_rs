import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func createAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentRemovalProposalDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentRemovalProposal {
        try validateOwnerRoomAgent(
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: proposerAgentID
        )
        try AgentGroupChatValidation.identifier(sourceDeliveryID, field: "sourceDeliveryID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try draft.validate()
        guard proposerAgentID != draft.targetAgentID else {
            throw AgentGroupChatError.permissionDenied
        }
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: roomID)?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: proposerAgentID
                  )?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: draft.targetAgentID
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
            if let existing = try readRemovalProposal(
                ownerUserID: ownerUserID,
                roomID: roomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            let proposal = LocalAgentRemovalProposal(
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
                INSERT INTO local_agent_removal_proposals (
                    owner_user_id, id, room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?, NULL)
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

    public func listAgentRemovalProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalAgentRemovalProposalStatus? = nil
    ) throws -> [LocalAgentRemovalProposal] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentProposalRepository.listRemovalProposals(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func approveAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentRemovalProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active,
                  let proposal = try readRemovalProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ), proposal.status == .pending,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: proposal.draft.targetAgentID
                  )?.status == .active else {
                throw AgentGroupChatError.conflict
            }
            guard room.projectManagerAgentID != proposal.draft.targetAgentID else {
                // Project-management authority must be handed over explicitly before removal.
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = CASE WHEN status = 'pending' THEN 'cancelled' ELSE 'failed' END,
                    last_error = ?, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND room_id = ? AND target_agent_id = ?
                  AND status IN ('pending', 'running')
                """,
                [
                    .text("成员已由 Human 确认移出当前团队。"), .integer(nowUnixMs),
                    .text(ownerUserID), .text(roomID), .text(proposal.draft.targetAgentID),
                ]
            )
            try execute(
                """
                UPDATE project_agent_room_members SET status = 'removed'
                WHERE owner_user_id = ? AND room_id = ? AND agent_id = ? AND status = 'active'
                """,
                [.text(ownerUserID), .text(roomID), .text(proposal.draft.targetAgentID)]
            )
            guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            if room.defaultAgentID == proposal.draft.targetAgentID {
                let replacement = try AgentConversationRepository.firstActiveMemberID(
                    database,
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    preparedStatement: recordPreparedStatement
                )
                try execute(
                    """
                    UPDATE project_agent_rooms SET default_agent_id = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND id = ? AND status = 'active'
                    """,
                    [
                        .optionalText(replacement), .integer(nowUnixMs),
                        .text(ownerUserID), .text(roomID),
                    ]
                )
            }
            try execute(
                """
                UPDATE local_agent_removal_proposals
                SET status = 'approved', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(proposalID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let approved = try readRemovalProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return approved
        }
    }

    public func rejectAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentRemovalProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_agent_removal_proposals
                SET status = 'rejected', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(proposalID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let rejected = try readRemovalProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return rejected
        }
    }

}
