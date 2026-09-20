import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func saveRun(_ run: LocalAgentGroupChatRun) throws {
        try run.validate()
        let context = run.context
        guard let delivery = try readDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), delivery.roomID == context.roomID,
           delivery.targetAgentID == context.agentID,
           delivery.messageID == context.triggerMessageID,
           delivery.rootMessageID == context.rootMessageID else {
            throw AgentGroupChatError.conflict
        }
        try transaction {
            if let existing = try readRun(
                ownerUserID: context.ownerUserID,
                deliveryID: context.deliveryID
            ), existing.id != run.id || existing.context != context
                || existing.createdAtUnixMs != run.createdAtUnixMs {
                throw AgentGroupChatError.conflict
            }
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let json = String(decoding: try encoder.encode(run), as: UTF8.self)
            try execute(
                """
                INSERT INTO local_agent_group_chat_runs (
                    owner_user_id, id, delivery_id, room_id, project_id, agent_id,
                    status, run_json, created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(owner_user_id, id) DO UPDATE SET
                    status = excluded.status,
                    run_json = excluded.run_json,
                    updated_at_unix_ms = excluded.updated_at_unix_ms
                """,
                [
                    .text(context.ownerUserID), .text(run.id.uuidString.lowercased()),
                    .text(context.deliveryID), .text(context.roomID), .text(context.projectID),
                    .text(context.agentID), .text(run.checkpoint.status.rawValue), .text(json),
                    .integer(run.createdAtUnixMs), .integer(run.updatedAtUnixMs),
                ]
            )
        }
    }

    public func run(
        ownerUserID: String,
        deliveryID: String
    ) throws -> LocalAgentGroupChatRun? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        return try readRun(ownerUserID: ownerUserID, deliveryID: deliveryID)
    }

    public func listUnfinishedRuns(
        ownerUserID: String,
        projectID: String,
        limit: Int
    ) throws -> [LocalAgentGroupChatRun] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        guard (1...500).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        return try AgentRunRepository.listUnfinished(
            database,
            ownerUserID: ownerUserID,
            projectID: projectID,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

    /// Trigger Runs belong to an Agent, independent of whether their source is a private chat,
    /// team message, heartbeat, Todo, or Todo status change.
    public func listAgentRuns(
        ownerUserID: String,
        agentID: String,
        limit: Int
    ) throws -> [LocalAgentGroupChatRun] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        guard (1...500).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        return try AgentRunRepository.listForAgent(
            database,
            ownerUserID: ownerUserID,
            agentID: agentID,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

    /// Recent execution history for a team, independent of member count.
    public func listRoomRuns(
        ownerUserID: String,
        roomID: String,
        limit: Int
    ) throws -> [LocalAgentGroupChatRun] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard (1...500).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        return try AgentRunRepository.listForRoom(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

}
