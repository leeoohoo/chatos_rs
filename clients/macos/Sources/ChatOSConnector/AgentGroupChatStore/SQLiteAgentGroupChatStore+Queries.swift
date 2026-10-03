import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    func readActiveRoom(ownerUserID: String, projectID: String) throws -> ProjectAgentRoom? {
        try AgentConversationRepository.activeProjectRoom(
            database,
            ownerUserID: ownerUserID,
            projectID: projectID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readDirectRoom(ownerUserID: String, directKey: String) throws -> ProjectAgentRoom? {
        try AgentConversationRepository.directRoom(
            database,
            ownerUserID: ownerUserID,
            directKey: directKey,
            preparedStatement: recordPreparedStatement
        )
    }

    func readRoom(ownerUserID: String, roomID: String) throws -> ProjectAgentRoom? {
        try AgentConversationRepository.room(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readAgent(ownerUserID: String, agentID: String) throws -> LocalAgentProfile? {
        try AgentProfileRepository.find(
            database,
            ownerUserID: ownerUserID,
            agentID: agentID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readMember(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) throws -> ProjectAgentRoomMember? {
        try AgentConversationRepository.member(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: agentID,
            preparedStatement: recordPreparedStatement
        )
    }

    func insertConversation(_ room: ProjectAgentRoom) throws {
        try AgentConversationRepository.insertRooms(
            database,
            rooms: [room],
            preparedStatement: recordPreparedStatement
        )
    }

    func insertDirectMember(
        ownerUserID: String,
        roomID: String,
        agent: LocalAgentProfile,
        nowUnixMs: Int64
    ) throws {
        try insertDirectMember(
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: agent.id,
            name: agent.draft.name,
            description: agent.draft.description,
            nowUnixMs: nowUnixMs
        )
    }

    func insertDirectMember(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        name: String,
        description: String,
        nowUnixMs: Int64
    ) throws {
        try AgentConversationRepository.insertMembers(
            database,
            members: [.init(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID,
                draft: .init(role: name, responsibility: description),
                joinedAtUnixMs: nowUnixMs
            )],
            preparedStatement: recordPreparedStatement
        )
    }

    func readMessage(ownerUserID: String, messageID: String) throws -> ProjectAgentMessage? {
        try AgentMessageRepository.find(
            database,
            ownerUserID: ownerUserID,
            messageID: messageID,
            preparedStatement: recordPreparedStatement,
            row: readMessage
        )
    }

    func readCursor(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) throws -> ProjectAgentReadCursor? {
        try AgentReadCursorRepository.find(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: agentID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) throws -> LocalAgentTodo? {
        try AgentTodoRepository.find(
            database,
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String
    ) throws -> LocalAgentCreationProposal? {
        try AgentProposalRepository.creationProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposalID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String
    ) throws -> LocalProjectCreationProposal? {
        try AgentProposalRepository.projectProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposalID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String
    ) throws -> LocalAgentRemovalProposal? {
        try AgentProposalRepository.removalProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposalID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String
    ) throws -> LocalAgentTeamCreationProposal? {
        try AgentProposalRepository.teamProposal(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            proposalID: proposalID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String
    ) throws -> LocalAgentMembershipProposal? {
        try AgentProposalRepository.membershipProposal(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            proposalID: proposalID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String
    ) throws -> LocalAgentTeamCreationProposal? {
        try AgentProposalRepository.teamProposal(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            proposerAgentID: proposerAgentID,
            sourceDeliveryID: sourceDeliveryID,
            requestKey: requestKey,
            preparedStatement: recordPreparedStatement
        )
    }

    func readMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String
    ) throws -> LocalAgentMembershipProposal? {
        try AgentProposalRepository.membershipProposal(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            proposerAgentID: proposerAgentID,
            sourceDeliveryID: sourceDeliveryID,
            requestKey: requestKey,
            preparedStatement: recordPreparedStatement
        )
    }

    func readRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String
    ) throws -> LocalAgentRemovalProposal? {
        try AgentProposalRepository.removalProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposerAgentID: proposerAgentID,
            sourceDeliveryID: sourceDeliveryID,
            requestKey: requestKey,
            preparedStatement: recordPreparedStatement
        )
    }

    func readProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String
    ) throws -> LocalProjectCreationProposal? {
        try AgentProposalRepository.projectProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposerAgentID: proposerAgentID,
            sourceDeliveryID: sourceDeliveryID,
            requestKey: requestKey,
            preparedStatement: recordPreparedStatement
        )
    }

    func readProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String
    ) throws -> LocalAgentCreationProposal? {
        try AgentProposalRepository.creationProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposerAgentID: proposerAgentID,
            sourceDeliveryID: sourceDeliveryID,
            requestKey: requestKey,
            preparedStatement: recordPreparedStatement
        )
    }

    func readDelivery(ownerUserID: String, deliveryID: String) throws -> ProjectAgentDelivery? {
        try AgentDeliveryRepository.delivery(
            database,
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            preparedStatement: recordPreparedStatement
        )
    }

    func readDelivery(
        ownerUserID: String,
        deduplicationKey: String
    ) throws -> ProjectAgentDelivery? {
        try AgentDeliveryRepository.delivery(
            database,
            ownerUserID: ownerUserID,
            deduplicationKey: deduplicationKey,
            preparedStatement: recordPreparedStatement
        )
    }

    func readRun(
        ownerUserID: String,
        deliveryID: String
    ) throws -> LocalAgentGroupChatRun? {
        try AgentRunRepository.run(
            database,
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            preparedStatement: recordPreparedStatement
        )
    }

}
