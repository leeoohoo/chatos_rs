import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    func hasOutstandingDeliveries(ownerUserID: String) throws -> Bool {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentDeliveryRepository.hasOutstandingDeliveries(
            database,
            ownerUserID: ownerUserID,
            preparedStatement: recordPreparedStatement
        )
    }

    func hasPendingDeliveries(
        ownerUserID: String,
        roomID: String? = nil,
        lane: LocalAgentRunLane? = nil
    ) throws -> Bool {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        if let roomID {
            try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        }
        return try AgentDeliveryRepository.hasPendingDeliveries(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            lane: lane,
            preparedStatement: recordPreparedStatement
        )
    }

    public func message(
        ownerUserID: String,
        roomID: String,
        messageID: String
    ) throws -> ProjectAgentMessage? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(messageID, field: "messageID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        let message = try readMessage(ownerUserID: ownerUserID, messageID: messageID)
        guard message?.roomID == roomID else { return nil }
        return message
    }

    /// Loads message context for a run list in one query instead of issuing one query per run.
    public func messages(
        ownerUserID: String,
        messageIDs: [String]
    ) throws -> [String: ProjectAgentMessage] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        let ids = Array(Set(messageIDs)).sorted()
        guard ids.count <= 500 else { throw AgentGroupChatError.invalidField("messageIDs") }
        for id in ids {
            try AgentGroupChatValidation.identifier(id, field: "messageID")
        }
        let messages = try hydrateMessageRelations(AgentMessageRepository.findMany(
            database,
            ownerUserID: ownerUserID,
            messageIDs: ids,
            preparedStatement: recordPreparedStatement,
            row: readMessageBase
        ), ownerUserID: ownerUserID)
        return Dictionary(uniqueKeysWithValues: messages.map { ($0.id, $0) })
    }

    public func claimNextDelivery(
        ownerUserID: String,
        agentID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try claimNextDelivery(
            ownerUserID: ownerUserID,
            roomID: nil,
            agentID: agentID,
            lane: nil,
            nowUnixMs: nowUnixMs
        )
    }

    public func claimNextDelivery(
        ownerUserID: String,
        agentID: String,
        lane: LocalAgentRunLane,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try claimNextDelivery(
            ownerUserID: ownerUserID,
            roomID: nil,
            agentID: agentID,
            lane: lane,
            nowUnixMs: nowUnixMs
        )
    }

    public func claimNextDelivery(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        return try claimNextDelivery(
            ownerUserID: ownerUserID,
            roomID: Optional(roomID),
            agentID: agentID,
            lane: nil,
            nowUnixMs: nowUnixMs
        )
    }

    public func claimNextDelivery(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        lane: LocalAgentRunLane,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        return try claimNextDelivery(
            ownerUserID: ownerUserID,
            roomID: Optional(roomID),
            agentID: agentID,
            lane: lane,
            nowUnixMs: nowUnixMs
        )
    }

    private func claimNextDelivery(
        ownerUserID: String,
        roomID: String?,
        agentID: String,
        lane: LocalAgentRunLane?,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let id = try AgentDeliveryRepository.nextPendingDeliveryID(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                roomID: roomID,
                lane: lane,
                preparedStatement: recordPreparedStatement
            ) else { return nil }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'running', attempt = attempt + 1, claimed_at_unix_ms = ?,
                    completed_at_unix_ms = NULL, last_error = NULL
                WHERE owner_user_id = ? AND id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(id)]
            )
            guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            return try readDelivery(ownerUserID: ownerUserID, deliveryID: id)
        }
    }

    public func delivery(
        ownerUserID: String,
        deliveryID: String
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        return try readDelivery(ownerUserID: ownerUserID, deliveryID: deliveryID)
    }

    /// Loads delivery context for run lists in one query. UI refreshes must not scale as N+1
    /// SQLite round trips as historical runs accumulate.
    public func deliveries(
        ownerUserID: String,
        deliveryIDs: [String]
    ) throws -> [String: ProjectAgentDelivery] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        let ids = Array(Set(deliveryIDs)).sorted()
        guard ids.count <= 500 else { throw AgentGroupChatError.invalidField("deliveryIDs") }
        for id in ids {
            try AgentGroupChatValidation.identifier(id, field: "deliveryID")
        }
        guard !ids.isEmpty else { return [:] }
        let deliveries = try AgentDeliveryRepository.deliveries(
            database,
            ownerUserID: ownerUserID,
            deliveryIDs: ids,
            preparedStatement: recordPreparedStatement
        )
        return Dictionary(uniqueKeysWithValues: deliveries.map { ($0.id, $0) })
    }

    public func completeDelivery(
        ownerUserID: String,
        deliveryID: String,
        responseMessageID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery {
        try validateDeliveryMutation(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            nowUnixMs: nowUnixMs
        )
        try AgentGroupChatValidation.identifier(responseMessageID, field: "responseMessageID")
        return try transaction {
            guard let delivery = try readDelivery(ownerUserID: ownerUserID, deliveryID: deliveryID),
                  delivery.status == .running else {
                throw AgentGroupChatError.conflict
            }
            guard let response = try readMessage(ownerUserID: ownerUserID, messageID: responseMessageID),
                  response.roomID == delivery.roomID,
                  response.senderKind == .agent,
                  response.senderID == delivery.targetAgentID else {
                throw AgentGroupChatError.invalidField("responseMessageID")
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'completed', response_message_id = ?, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'running'
                """,
                [
                    .text(responseMessageID), .integer(nowUnixMs),
                    .text(ownerUserID), .text(deliveryID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readDelivery(ownerUserID: ownerUserID, deliveryID: deliveryID) else {
                throw AgentGroupChatError.conflict
            }
            return updated
        }
    }

    public func completeHeartbeatDelivery(
        ownerUserID: String,
        deliveryID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery {
        try validateDeliveryMutation(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            nowUnixMs: nowUnixMs
        )
        return try transaction {
            guard let delivery = try readDelivery(
                ownerUserID: ownerUserID,
                deliveryID: deliveryID
            ), delivery.status == .running else {
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'completed', response_message_id = NULL, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'running'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(deliveryID)]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: deliveryID
                  ) else { throw AgentGroupChatError.conflict }
            return updated
        }
    }

    public func failDelivery(
        ownerUserID: String,
        deliveryID: String,
        error: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery {
        try validateDeliveryMutation(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            nowUnixMs: nowUnixMs
        )
        try AgentGroupChatValidation.text(error, field: "error", maximumLength: 8_000)
        return try transaction {
            guard let delivery = try readDelivery(
                ownerUserID: ownerUserID,
                deliveryID: deliveryID
            ), delivery.status == .running else {
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'failed', last_error = ?, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'running'
                """,
                [.text(error), .integer(nowUnixMs), .text(ownerUserID), .text(deliveryID)]
            )
            guard sqlite3_changes(database) == 1 else {
                throw AgentGroupChatError.conflict
            }
            if let todoID = delivery.todoID {
                try execute(
                    """
                    UPDATE local_agent_todos
                    SET status = 'blocked', blocked_reason = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND agent_id = ? AND id = ?
                    """,
                    [
                        .text(error), .integer(nowUnixMs), .text(ownerUserID),
                        .text(delivery.targetAgentID), .text(todoID),
                    ]
                )
            }
            guard let updated = try readDelivery(
                ownerUserID: ownerUserID,
                deliveryID: deliveryID
            ) else {
                throw AgentGroupChatError.conflict
            }
            return updated
        }
    }

    /// A manager Run that stopped for Human review must not monopolize the Agent's communication
    /// lane. Its checkpoint remains `needsReview`, while the delivery leaves the running queue so
    /// later Human messages, Todo notifications and heartbeats can continue.
    func quarantineNeedsReviewManagerDeliveries(
        ownerUserID: String,
        nowUnixMs: Int64
    ) throws -> Int {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'failed', last_error = ?, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND status = 'running' AND trigger_kind != 'todo'
                  AND EXISTS (
                      SELECT 1 FROM local_agent_group_chat_runs run
                      WHERE run.owner_user_id = project_agent_deliveries.owner_user_id
                        AND run.delivery_id = project_agent_deliveries.id
                        AND run.status = ?
                  )
                """,
                [
                    .text("通讯 Run 中断，等待用户检查；后续通讯仍会继续。"),
                    .integer(nowUnixMs), .text(ownerUserID),
                    .text(AgentRunCheckpoint.Status.needsReview.rawValue),
                ]
            )
            return Int(sqlite3_changes(database))
        }
    }

    func reactivateManagerDeliveryForReview(
        ownerUserID: String,
        deliveryID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery {
        try validateDeliveryMutation(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            nowUnixMs: nowUnixMs
        )
        return try transaction {
            guard let delivery = try readDelivery(
                ownerUserID: ownerUserID,
                deliveryID: deliveryID
            ), delivery.status == .failed, delivery.lane == .manager else {
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'running', last_error = NULL, completed_at_unix_ms = NULL,
                    claimed_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'failed'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(deliveryID)]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: deliveryID
                  ) else { throw AgentGroupChatError.conflict }
            return updated
        }
    }

    /// Atomically stops every queued or running delivery in one room. Pending work is cancelled;
    /// running work is failed and any durable Run is closed in the same SQLite transaction.
    public func stopOutstandingDeliveries(
        ownerUserID: String,
        roomID: String,
        reason: String,
        nowUnixMs: Int64
    ) throws -> Int {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.text(reason, field: "reason", maximumLength: 8_000)
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
                throw AgentGroupChatError.notFound
            }
            let deliveryIDs = try AgentDeliveryRepository.outstandingDeliveryIDs(
                database,
                ownerUserID: ownerUserID,
                roomID: roomID,
                preparedStatement: recordPreparedStatement
            )
            guard !deliveryIDs.isEmpty else { return 0 }

            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let runs = try AgentRunRepository.listForOutstandingDeliveries(
                database,
                ownerUserID: ownerUserID,
                roomID: roomID,
                preparedStatement: recordPreparedStatement
            )
            var failedRunUpdates: [AgentRunRepository.FailedRunUpdate] = []
            for var run in runs {
                run.checkpoint.status = .failed
                run.checkpoint.stopReason = reason
                run.events.append(.init(
                    kind: "stopped_all",
                    detail: reason,
                    modelCalls: run.checkpoint.modelCalls
                ))
                run.updatedAtUnixMs = max(nowUnixMs, run.updatedAtUnixMs)
                try run.validate()
                let json = String(decoding: try encoder.encode(run), as: UTF8.self)
                failedRunUpdates.append(.init(
                    id: run.id,
                    json: json,
                    updatedAtUnixMs: run.updatedAtUnixMs
                ))
            }
            let updatedRunCount = try AgentRunRepository.markFailed(
                database,
                ownerUserID: ownerUserID,
                updates: failedRunUpdates,
                preparedStatement: recordPreparedStatement
            )
            guard updatedRunCount == failedRunUpdates.count else {
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = CASE status WHEN 'running' THEN 'failed' ELSE 'cancelled' END,
                    last_error = ?, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND room_id = ? AND status IN ('pending', 'running')
                """,
                [.text(reason), .integer(nowUnixMs), .text(ownerUserID), .text(roomID)]
            )
            guard sqlite3_changes(database) == Int32(deliveryIDs.count) else {
                throw AgentGroupChatError.conflict
            }
            return deliveryIDs.count
        }
    }

}
