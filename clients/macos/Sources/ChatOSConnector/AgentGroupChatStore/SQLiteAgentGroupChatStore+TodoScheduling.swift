import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func enqueuePendingAgentTodos(
        ownerUserID: String,
        nowUnixMs: Int64,
        agentLimit: Int = 32
    ) throws -> [ProjectAgentDelivery] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        guard nowUnixMs >= 0, (1...128).contains(agentLimit) else {
            throw AgentGroupChatError.invalidField("agentLimit")
        }
        return try transaction {
            let agentIDs = try AgentTodoRepository.pendingAgentIDs(
                database,
                ownerUserID: ownerUserID,
                limit: agentLimit,
                preparedStatement: recordPreparedStatement
            )
            var deliveries: [ProjectAgentDelivery] = []
            for agentID in agentIDs {
                let outstanding = try AgentDeliveryRepository.outstandingCount(
                    database,
                    ownerUserID: ownerUserID,
                    targetAgentID: agentID,
                    triggerKind: .todo,
                    preparedStatement: recordPreparedStatement
                )
                guard outstanding == 0 else { continue }
                guard let todo = try AgentTodoRepository.nextReady(
                    database,
                    ownerUserID: ownerUserID,
                    agentID: agentID,
                    preparedStatement: recordPreparedStatement
                ) else { continue }
                let delivery = try prepareTodoDelivery(
                    ownerUserID: ownerUserID,
                    agentID: agentID,
                    todo: todo,
                    nowUnixMs: nowUnixMs
                )
                try execute(
                    """
                    UPDATE local_agent_todos
                    SET status = 'in_progress', updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND agent_id = ? AND id = ? AND status = 'pending'
                    """,
                    [.integer(nowUnixMs), .text(ownerUserID), .text(agentID), .text(todo.id)]
                )
                guard sqlite3_changes(database) == 1 else {
                    throw AgentGroupChatError.conflict
                }
                deliveries.append(delivery)
            }
            return deliveries
        }
    }

    public func agentTodoScheduleState(
        ownerUserID: String,
        agentID: String
    ) throws -> LocalAgentTodoScheduleState {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        return try transaction {
            guard try readAgent(ownerUserID: ownerUserID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let running = try AgentTodoRepository.running(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                preparedStatement: recordPreparedStatement
            )
            let outstanding = try AgentDeliveryRepository.outstandingCount(
                database,
                ownerUserID: ownerUserID,
                targetAgentID: agentID,
                triggerKind: .todo,
                preparedStatement: recordPreparedStatement
            )
            // Match startNextReadyAgentTodo exactly: a Todo is only advertised as ready when
            // the executor lane can actually start it. This prevents manager cycles from being
            // trapped between `ready_todo_requires_start` and `no_ready_todo`.
            let ready: LocalAgentTodo? = if running == nil, outstanding == 0 {
                try AgentTodoRepository.nextReady(
                    database,
                    ownerUserID: ownerUserID,
                    agentID: agentID,
                    preparedStatement: recordPreparedStatement
                )
            } else {
                nil
            }
            return .init(runningTodo: running, readyTodo: ready)
        }
    }

    /// Explicit manager-cycle scheduling entry point. Unlike the legacy account-wide enqueue
    /// helper, this starts work only for the authenticated current Agent and does so atomically.
    public func startNextReadyAgentTodo(
        ownerUserID: String,
        agentID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readAgent(ownerUserID: ownerUserID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let outstanding = try AgentDeliveryRepository.outstandingCount(
                database,
                ownerUserID: ownerUserID,
                targetAgentID: agentID,
                triggerKind: .todo,
                preparedStatement: recordPreparedStatement
            )
            let runningTodoCount = try AgentTodoRepository.runningCount(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                preparedStatement: recordPreparedStatement
            )
            guard outstanding == 0, runningTodoCount == 0 else { return nil }
            guard let todo = try AgentTodoRepository.nextReady(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                preparedStatement: recordPreparedStatement
            ) else { return nil }

            let delivery = try prepareTodoDelivery(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todo: todo,
                nowUnixMs: nowUnixMs
            )
            try execute(
                """
                INSERT OR IGNORE INTO local_agent_todo_asset_snapshots (
                    owner_user_id, todo_id, asset_id, team_room_id, category,
                    title, markdown, revision, captured_at_unix_ms
                )
                SELECT owner_user_id, ?, id, team_room_id, category,
                       title, markdown, revision, ?
                FROM local_agent_team_assets
                WHERE owner_user_id = ? AND team_room_id = ? AND status = 'active'
                """,
                [
                    .text(todo.id), .integer(nowUnixMs), .text(ownerUserID),
                    .text(todo.teamRoomID),
                ]
            )
            try execute(
                """
                UPDATE local_agent_todos
                SET status = 'in_progress', updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND agent_id = ? AND id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(agentID), .text(todo.id)]
            )
            guard sqlite3_changes(database) == 1 else {
                throw AgentGroupChatError.conflict
            }
            return delivery
        }
    }

    /// A Todo delivery owns one durable executor Run. Retrying a failed Todo must therefore
    /// reactivate that delivery instead of inserting another row with the same deduplication key
    /// or silently creating a fresh Run that loses the saved checkpoint.
    private func prepareTodoDelivery(
        ownerUserID: String,
        agentID: String,
        todo: LocalAgentTodo,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery {
        let deduplicationKey = "todo:\(todo.id)"
        if let existing = try readDelivery(
            ownerUserID: ownerUserID,
            deduplicationKey: deduplicationKey
        ) {
            guard existing.triggerKind == .todo,
                  existing.targetAgentID == agentID,
                  existing.status == .failed else {
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE project_agent_messages
                SET content = ?
                WHERE owner_user_id = ? AND id = ?
                """,
                [
                    .text(todo.detail.isEmpty ? todo.title : "\(todo.title)\n\n\(todo.detail)"),
                    .text(ownerUserID), .text(existing.messageID),
                ]
            )
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'pending', response_message_id = NULL, last_error = NULL,
                    claimed_at_unix_ms = NULL, completed_at_unix_ms = NULL
                WHERE owner_user_id = ? AND id = ? AND status = 'failed'
                """,
                [.text(ownerUserID), .text(existing.id)]
            )
            guard sqlite3_changes(database) == 1,
                  let reactivated = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: existing.id
                  ) else {
                throw AgentGroupChatError.conflict
            }
            return reactivated
        }

        let messageID = UUID().uuidString.lowercased()
        let deliveryID = UUID().uuidString.lowercased()
        try execute(
            """
            INSERT INTO project_agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_id, content,
                reply_to_message_id, source_run_id, causation_id, root_message_id,
                hop_count, created_at_unix_ms
            ) VALUES (?, ?, ?, 'system', 'system', ?, NULL, NULL, 'todo', ?, 0, ?)
            """,
            [
                .text(ownerUserID), .text(messageID), .text(todo.teamRoomID),
                .text(todo.detail.isEmpty ? todo.title : "\(todo.title)\n\n\(todo.detail)"),
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
            ) VALUES (?, ?, ?, ?, ?, ?, 'todo', 'pending', 0, 0, ?, NULL, NULL, NULL, NULL, ?)
            """,
            [
                .text(ownerUserID), .text(deliveryID), .text(todo.teamRoomID),
                .text(messageID), .text(messageID), .text(agentID),
                .text(deduplicationKey), .integer(nowUnixMs),
            ]
        )
        guard let delivery = try readDelivery(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
        ) else {
            throw AgentGroupChatError.storage("Todo delivery insert failed")
        }
        return delivery
    }


    public func agentTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) throws -> LocalAgentTodo? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        return try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID)
    }

    public func todoForDelivery(
        ownerUserID: String,
        deliveryID: String
    ) throws -> LocalAgentTodo? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        guard let delivery = try readDelivery(ownerUserID: ownerUserID, deliveryID: deliveryID),
              delivery.triggerKind == .todo,
              delivery.deduplicationKey.hasPrefix("todo:") else { return nil }
        return try readTodo(
            ownerUserID: ownerUserID,
            agentID: delivery.targetAgentID,
            todoID: String(delivery.deduplicationKey.dropFirst("todo:".count))
        )
    }

    public func enqueueAgentTodoReady(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        guard let candidate = try readTodo(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID
        ), candidate.status == .pending else { return nil }
        let room = try openHumanAgentDirect(ownerUserID: ownerUserID, agentID: agentID)
        return try transaction {
            guard let todo = try readTodo(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID
            ), todo.status == .pending else { return nil }
            let incomplete = try AgentTodoRepository.incompleteDependencyCount(
                database,
                ownerUserID: ownerUserID,
                todoID: todoID,
                preparedStatement: recordPreparedStatement
            )
            guard incomplete == 0 else { return nil }
            let eventKey = "ready:\(todo.id):\(todo.updatedAtUnixMs)"
            let key = "todo-ready:\(todo.id):\(todo.updatedAtUnixMs):\(agentID)"
            if let existing = try readDelivery(
                ownerUserID: ownerUserID,
                deduplicationKey: key
            ) {
                try insertTodoEventRecipient(
                    ownerUserID: ownerUserID,
                    eventKey: eventKey,
                    todoID: todo.id,
                    eventKind: "ready",
                    recipientAgentID: agentID,
                    deliveryID: existing.id,
                    messageID: existing.messageID,
                    nowUnixMs: nowUnixMs
                )
                return existing
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
                    .text(messageID), .text(agentID), .text(key), .integer(nowUnixMs),
                ]
            )
            guard let delivery = try readDelivery(
                ownerUserID: ownerUserID,
                deliveryID: deliveryID
            ) else { throw AgentGroupChatError.storage("Todo ready delivery insert failed") }
            try insertTodoEventRecipient(
                ownerUserID: ownerUserID,
                eventKey: eventKey,
                todoID: todo.id,
                eventKind: "ready",
                recipientAgentID: agentID,
                deliveryID: delivery.id,
                messageID: messageID,
                nowUnixMs: nowUnixMs
            )
            return delivery
        }
    }

    public func enqueueReadyDependentAgentTodos(
        ownerUserID: String,
        prerequisiteTodoID: String,
        nowUnixMs: Int64
    ) throws -> [ProjectAgentDelivery] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(prerequisiteTodoID, field: "prerequisiteTodoID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        let dependents = try AgentTodoRepository.pendingDependents(
            database,
            ownerUserID: ownerUserID,
            prerequisiteTodoID: prerequisiteTodoID,
            preparedStatement: recordPreparedStatement
        )
        var deliveries: [ProjectAgentDelivery] = []
        for dependent in dependents {
            if let delivery = try enqueueAgentTodoReady(
                ownerUserID: ownerUserID,
                agentID: dependent.agentID,
                todoID: dependent.todoID,
                nowUnixMs: nowUnixMs
            ) {
                deliveries.append(delivery)
            }
        }
        return deliveries
    }

    public func enqueueAgentTodoStatus(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        excludingAgentID: String?,
        nowUnixMs: Int64
    ) throws -> [ProjectAgentDelivery] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        guard let statusTodo = try readTodo(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID
        ), let team = try readRoom(
            ownerUserID: ownerUserID,
            roomID: statusTodo.teamRoomID
        ) else {
            throw AgentGroupChatError.conflict
        }
        // Legacy trusted callers may have Todo rows created before teams acquired an explicit
        // project-manager binding. Model-facing creation always supplies `creatorAgentID` and
        // therefore cannot reach this compatibility fallback.
        let projectManagerAgentID = team.projectManagerAgentID ?? agentID
        if let excludingAgentID {
            try AgentGroupChatValidation.identifier(
                excludingAgentID,
                field: "excludingAgentID"
            )
        }
        let recipientIDs = Array(Set([agentID, projectManagerAgentID]))
            .filter { $0 != excludingAgentID }
            .sorted()
        var roomsByAgentID: [String: ProjectAgentRoom] = [:]
        for recipientID in recipientIDs {
            roomsByAgentID[recipientID] = try openHumanAgentDirect(
                ownerUserID: ownerUserID,
                agentID: recipientID
            )
        }
        return try transaction {
            guard let todo = try readTodo(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID
            ), todo.status == .completed || todo.status == .blocked || todo.status == .cancelled else {
                throw AgentGroupChatError.conflict
            }
            let summary = todo.status == .completed ? todo.result : todo.blockedReason
            let latestProgress = try AgentTodoRepository.listProgress(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID,
                limit: 1,
                preparedStatement: recordPreparedStatement
            ).last
            let suggestionCount = latestProgress?.assetUpdateSuggestions.count ?? 0
            let suggestionNotice = suggestionCount > 0
                ? "\n共享资产更新建议：\(suggestionCount) 条，请用 todo_read_progress 审核后决定是否落库。"
                : ""
            let content = "Todo 状态已更新：\(todo.title)\n状态：\(todo.status.rawValue)\n\(summary)\(suggestionNotice)"
            let eventKey = "status:\(todo.id):\(todo.status.rawValue):\(todo.updatedAtUnixMs)"
            var deliveries: [ProjectAgentDelivery] = []
            for recipientID in recipientIDs {
                guard let room = roomsByAgentID[recipientID] else {
                    throw AgentGroupChatError.storage("Todo status room is missing")
                }
                let key = "todo-status:\(todo.id):\(todo.status.rawValue):\(todo.updatedAtUnixMs):\(recipientID)"
                if let existing = try readDelivery(
                    ownerUserID: ownerUserID,
                    deduplicationKey: key
                ) {
                    try insertTodoEventRecipient(
                        ownerUserID: ownerUserID,
                        eventKey: eventKey,
                        todoID: todo.id,
                        eventKind: todo.status.rawValue,
                        recipientAgentID: recipientID,
                        deliveryID: existing.id,
                        messageID: existing.messageID,
                        nowUnixMs: nowUnixMs
                    )
                    deliveries.append(existing)
                    continue
                }
                let messageID = UUID().uuidString.lowercased()
                let deliveryID = UUID().uuidString.lowercased()
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
                        .text(messageID), .text(recipientID), .text(key), .integer(nowUnixMs),
                    ]
                )
                guard let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: deliveryID
                ) else { throw AgentGroupChatError.storage("Todo status delivery insert failed") }
                try insertTodoEventRecipient(
                    ownerUserID: ownerUserID,
                    eventKey: eventKey,
                    todoID: todo.id,
                    eventKind: todo.status.rawValue,
                    recipientAgentID: recipientID,
                    deliveryID: delivery.id,
                    messageID: messageID,
                    nowUnixMs: nowUnixMs
                )
                deliveries.append(delivery)
            }
            return deliveries
        }
    }

    private func insertTodoEventRecipient(
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

}
