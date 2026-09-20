import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func createProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalProjectCreationProposalDraft,
        nowUnixMs: Int64
    ) throws -> LocalProjectCreationProposal {
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
                  let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: sourceDeliveryID
                  ), delivery.roomID == roomID,
                     delivery.targetAgentID == proposerAgentID,
                     delivery.status == .running else {
                throw AgentGroupChatError.permissionDenied
            }
            if let existing = try readProjectProposal(
                ownerUserID: ownerUserID,
                roomID: roomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            let proposal = LocalProjectCreationProposal(
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
                INSERT INTO local_project_creation_proposals (
                    owner_user_id, id, room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_project_id,
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

    public func listProjectProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalProjectCreationProposalStatus? = nil
    ) throws -> [LocalProjectCreationProposal] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentProposalRepository.listProjectProposals(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func approveProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        createdProjectID: String,
        nowUnixMs: Int64
    ) throws -> LocalProjectCreationProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        try AgentGroupChatValidation.identifier(createdProjectID, field: "createdProjectID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_project_creation_proposals
                SET status = 'approved', created_project_id = ?, resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [
                    .text(createdProjectID), .integer(nowUnixMs), .text(ownerUserID),
                    .text(proposalID), .text(roomID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let approved = try readProjectProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return approved
        }
    }

    public func rejectProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalProjectCreationProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_project_creation_proposals
                SET status = 'rejected', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(proposalID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let rejected = try readProjectProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return rejected
        }
    }

}
