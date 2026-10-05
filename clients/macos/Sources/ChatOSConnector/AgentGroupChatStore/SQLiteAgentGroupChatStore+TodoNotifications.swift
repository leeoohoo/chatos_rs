import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension SQLiteAgentGroupChatStore {
    func insertTodoEventRecipient(
        ownerUserID: String,
        eventKey: String,
        todoID: String,
        eventKind: String,
        recipientAgentID: String,
        deliveryID: String,
        messageID: String,
        nowUnixMs: Int64
    ) throws {
        try execute(
            """
            INSERT OR IGNORE INTO local_agent_todo_event_recipients (
                owner_user_id, event_key, todo_id, event_kind, recipient_agent_id,
                delivery_id, message_id, created_at_unix_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            [
                .text(ownerUserID), .text(eventKey), .text(todoID), .text(eventKind),
                .text(recipientAgentID), .text(deliveryID), .text(messageID),
                .integer(nowUnixMs),
            ]
        )
    }

    func enqueueTodoReadyNotification(
        ownerUserID: String,
        todo: LocalAgentTodo,
        room: ProjectAgentRoom,
        deduplicationKey: String,
        existingDelivery: ProjectAgentDelivery?,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery {
        let eventKey = "ready:\(todo.id):\(todo.updatedAtUnixMs)"
        if let existingDelivery {
            try insertTodoEventRecipient(
                ownerUserID: ownerUserID,
                eventKey: eventKey,
                todoID: todo.id,
                eventKind: "ready",
                recipientAgentID: todo.agentID,
                deliveryID: existingDelivery.id,
                messageID: existingDelivery.messageID,
                nowUnixMs: nowUnixMs
            )
            return existingDelivery
        }

        let messageID = UUID().uuidString.lowercased()
        let deliveryID = UUID().uuidString.lowercased()
        let content = "Todo 已可执行：\(todo.title)\n优先级：\(todo.priority)"
        try execute(
            """
            INSERT INTO project_agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_id, content,
                reply_to_message_id, source_run_id, causation_id, root_message_id,
                hop_count, created_at_unix_ms
            ) VALUES (?, ?, ?, 'system', 'system', ?, NULL, NULL, 'todo_status', ?, 0, ?)
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
            ) VALUES (?, ?, ?, ?, ?, ?, 'todo_status', 'pending', 0, 0, ?, NULL, NULL, NULL, NULL, ?)
            """,
            [
                .text(ownerUserID), .text(deliveryID), .text(room.id), .text(messageID),
                .text(messageID), .text(todo.agentID), .text(deduplicationKey),
                .integer(nowUnixMs),
            ]
        )
        let delivery = ProjectAgentDelivery(
            id: deliveryID,
            ownerUserID: ownerUserID,
            roomID: room.id,
            messageID: messageID,
            rootMessageID: messageID,
            targetAgentID: todo.agentID,
            triggerKind: .todoStatus,
            status: .pending,
            attempt: 0,
            hopCount: 0,
            deduplicationKey: deduplicationKey,
            createdAtUnixMs: nowUnixMs
        )
        try insertTodoEventRecipient(
            ownerUserID: ownerUserID,
            eventKey: eventKey,
            todoID: todo.id,
            eventKind: "ready",
            recipientAgentID: todo.agentID,
            deliveryID: delivery.id,
            messageID: messageID,
            nowUnixMs: nowUnixMs
        )
        return delivery
    }

}
