import ChatOSCore
import SQLite3

enum AgentProposalRepository {
    static func listCreationProposals(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        status: LocalAgentCreationProposalStatus?,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentCreationProposal] {
        var sql = "SELECT \(creationColumns) FROM local_agent_creation_proposals WHERE owner_user_id = ? AND room_id = ?"
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(roomID)]
        if let status {
            sql += " AND status = ?"
            values.append(.text(status.rawValue))
        }
        sql += " ORDER BY created_at_unix_ms, id"
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            sql,
            values,
            row: AgentGroupChatRowMapper.agentProposal
        )
    }

    static func creationProposal(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentCreationProposal? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(creationColumns) FROM local_agent_creation_proposals
            WHERE owner_user_id = ? AND room_id = ? AND id = ? LIMIT 1
            """,
            [.text(ownerUserID), .text(roomID), .text(proposalID)],
            row: AgentGroupChatRowMapper.agentProposal
        ).first
    }

    static func creationProposal(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentCreationProposal? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(creationColumns) FROM local_agent_creation_proposals
            WHERE owner_user_id = ? AND room_id = ? AND proposer_agent_id = ?
              AND source_delivery_id = ? AND request_key = ? LIMIT 1
            """,
            [
                .text(ownerUserID), .text(roomID), .text(proposerAgentID),
                .text(sourceDeliveryID), .text(requestKey),
            ],
            row: AgentGroupChatRowMapper.agentProposal
        ).first
    }

    static func listRemovalProposals(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        status: LocalAgentRemovalProposalStatus?,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentRemovalProposal] {
        var sql = "SELECT \(removalColumns) FROM local_agent_removal_proposals WHERE owner_user_id = ? AND room_id = ?"
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(roomID)]
        if let status {
            sql += " AND status = ?"
            values.append(.text(status.rawValue))
        }
        sql += " ORDER BY created_at_unix_ms, id"
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            sql,
            values,
            row: AgentGroupChatRowMapper.removalProposal
        )
    }

    static func removalProposal(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentRemovalProposal? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(removalColumns) FROM local_agent_removal_proposals
            WHERE owner_user_id = ? AND room_id = ? AND id = ? LIMIT 1
            """,
            [.text(ownerUserID), .text(roomID), .text(proposalID)],
            row: AgentGroupChatRowMapper.removalProposal
        ).first
    }

    static func removalProposal(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentRemovalProposal? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(removalColumns) FROM local_agent_removal_proposals
            WHERE owner_user_id = ? AND room_id = ? AND proposer_agent_id = ?
              AND source_delivery_id = ? AND request_key = ? LIMIT 1
            """,
            [
                .text(ownerUserID), .text(roomID), .text(proposerAgentID),
                .text(sourceDeliveryID), .text(requestKey),
            ],
            row: AgentGroupChatRowMapper.removalProposal
        ).first
    }

    static func listMembershipProposals(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        sourceRoomID: String,
        status: LocalAgentMembershipProposalStatus?,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentMembershipProposal] {
        var sql = "SELECT \(membershipColumns) FROM local_agent_membership_proposals WHERE owner_user_id = ? AND source_room_id = ?"
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(sourceRoomID)]
        if let status {
            sql += " AND status = ?"
            values.append(.text(status.rawValue))
        }
        sql += " ORDER BY created_at_unix_ms, id"
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            sql,
            values,
            row: AgentGroupChatRowMapper.membershipProposal
        )
    }

    static func membershipProposal(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentMembershipProposal? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(membershipColumns) FROM local_agent_membership_proposals
            WHERE owner_user_id = ? AND source_room_id = ? AND id = ? LIMIT 1
            """,
            [.text(ownerUserID), .text(sourceRoomID), .text(proposalID)],
            row: AgentGroupChatRowMapper.membershipProposal
        ).first
    }

    static func membershipProposal(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentMembershipProposal? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(membershipColumns) FROM local_agent_membership_proposals
            WHERE owner_user_id = ? AND source_room_id = ? AND proposer_agent_id = ?
              AND source_delivery_id = ? AND request_key = ? LIMIT 1
            """,
            [
                .text(ownerUserID), .text(sourceRoomID), .text(proposerAgentID),
                .text(sourceDeliveryID), .text(requestKey),
            ],
            row: AgentGroupChatRowMapper.membershipProposal
        ).first
    }

    static func listTeamProposals(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        sourceRoomID: String,
        status: LocalAgentTeamCreationProposalStatus?,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTeamCreationProposal] {
        var sql = "SELECT \(teamColumns) FROM local_agent_team_creation_proposals WHERE owner_user_id = ? AND source_room_id = ?"
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(sourceRoomID)]
        if let status {
            sql += " AND status = ?"
            values.append(.text(status.rawValue))
        }
        sql += " ORDER BY created_at_unix_ms, id"
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            sql,
            values,
            row: AgentGroupChatRowMapper.teamProposal
        )
    }

    static func teamProposal(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTeamCreationProposal? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(teamColumns) FROM local_agent_team_creation_proposals
            WHERE owner_user_id = ? AND source_room_id = ? AND id = ? LIMIT 1
            """,
            [.text(ownerUserID), .text(sourceRoomID), .text(proposalID)],
            row: AgentGroupChatRowMapper.teamProposal
        ).first
    }

    static func teamProposal(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTeamCreationProposal? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(teamColumns) FROM local_agent_team_creation_proposals
            WHERE owner_user_id = ? AND source_room_id = ? AND proposer_agent_id = ?
              AND source_delivery_id = ? AND request_key = ? LIMIT 1
            """,
            [
                .text(ownerUserID), .text(sourceRoomID), .text(proposerAgentID),
                .text(sourceDeliveryID), .text(requestKey),
            ],
            row: AgentGroupChatRowMapper.teamProposal
        ).first
    }

    private static let creationColumns = "owner_user_id, id, room_id, proposer_agent_id, source_delivery_id, request_key, draft_json, status, created_agent_id, created_at_unix_ms, resolved_at_unix_ms"
    private static let removalColumns = "owner_user_id, id, room_id, proposer_agent_id, source_delivery_id, request_key, draft_json, status, created_at_unix_ms, resolved_at_unix_ms"
    private static let membershipColumns = "owner_user_id, id, source_room_id, proposer_agent_id, source_delivery_id, request_key, draft_json, status, created_at_unix_ms, resolved_at_unix_ms"
    private static let teamColumns = "owner_user_id, id, source_room_id, proposer_agent_id, source_delivery_id, request_key, draft_json, status, created_room_id, created_at_unix_ms, resolved_at_unix_ms"
}
