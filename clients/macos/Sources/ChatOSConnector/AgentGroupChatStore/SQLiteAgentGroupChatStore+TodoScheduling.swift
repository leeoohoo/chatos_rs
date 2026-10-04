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
            let todos = try AgentTodoRepository.nextReadyBatch(
                database,
                ownerUserID: ownerUserID,
                limit: agentLimit,
                preparedStatement: recordPreparedStatement
            )
            let keysByTodoID = Dictionary(uniqueKeysWithValues: todos.map {
                ($0.id, "todo:\($0.id)")
            })
            let existingDeliveries = try AgentDeliveryRepository.deliveries(
                database,
                ownerUserID: ownerUserID,
                deduplicationKeys: Array(keysByTodoID.values),
                preparedStatement: recordPreparedStatement
            )
            let existingByKey = Dictionary(
                uniqueKeysWithValues: existingDeliveries.map { ($0.deduplicationKey, $0) }
            )
            var newMessages: [ProjectAgentMessage] = []
            var newDeliveries: [ProjectAgentDelivery] = []
            var retryMessageUpdates: [(messageID: String, content: String)] = []
            var failedDeliveryIDs: [String] = []
            var deliveriesByTodoID: [String: ProjectAgentDelivery] = [:]
            for todo in todos {
                guard let key = keysByTodoID[todo.id] else {
                    throw AgentGroupChatError.storage("Todo delivery key is missing")
                }
                let content = todo.detail.isEmpty ? todo.title : "\(todo.title)\n\n\(todo.detail)"
                let delivery: ProjectAgentDelivery
                if let existing = existingByKey[key] {
                    guard existing.triggerKind == .todo,
                          existing.targetAgentID == todo.agentID,
                          existing.status == .failed else {
                        throw AgentGroupChatError.conflict
                    }
                    retryMessageUpdates.append((existing.messageID, content))
                    failedDeliveryIDs.append(existing.id)
                    delivery = .init(
                        id: existing.id,
                        ownerUserID: existing.ownerUserID,
                        roomID: existing.roomID,
                        messageID: existing.messageID,
                        rootMessageID: existing.rootMessageID,
                        targetAgentID: existing.targetAgentID,
                        triggerKind: existing.triggerKind,
                        status: .pending,
                        attempt: existing.attempt,
                        hopCount: existing.hopCount,
                        deduplicationKey: existing.deduplicationKey,
                        createdAtUnixMs: existing.createdAtUnixMs
                    )
                } else {
                    let messageID = UUID().uuidString.lowercased()
                    let deliveryID = UUID().uuidString.lowercased()
                    newMessages.append(.init(
                        id: messageID,
                        ownerUserID: ownerUserID,
                        roomID: todo.teamRoomID,
                        draft: .init(
                            senderKind: .system,
                            senderID: "system",
                            content: content,
                            causationID: "todo",
                            rootMessageID: messageID
                        ),
                        rootMessageID: messageID,
                        createdAtUnixMs: nowUnixMs
                    ))
                    delivery = .init(
                        id: deliveryID,
                        ownerUserID: ownerUserID,
                        roomID: todo.teamRoomID,
                        messageID: messageID,
                        rootMessageID: messageID,
                        targetAgentID: todo.agentID,
                        triggerKind: .todo,
                        status: .pending,
                        attempt: 0,
                        hopCount: 0,
                        deduplicationKey: key,
                        createdAtUnixMs: nowUnixMs
                    )
                    newDeliveries.append(delivery)
                }
                deliveriesByTodoID[todo.id] = delivery
            }
            try AgentMessageRepository.insert(
                database,
                messages: newMessages,
                preparedStatement: recordPreparedStatement
            )
            let updatedMessages = try AgentMessageRepository.updateContents(
                database,
                ownerUserID: ownerUserID,
                updates: retryMessageUpdates,
                preparedStatement: recordPreparedStatement
            )
            guard updatedMessages == retryMessageUpdates.count else {
                throw AgentGroupChatError.conflict
            }
            try AgentDeliveryRepository.insert(
                database,
                deliveries: newDeliveries,
                preparedStatement: recordPreparedStatement
            )
            let reactivated = try AgentDeliveryRepository.reactivateFailed(
                database,
                ownerUserID: ownerUserID,
                deliveryIDs: failedDeliveryIDs,
                preparedStatement: recordPreparedStatement
            )
            guard reactivated == failedDeliveryIDs.count else {
                throw AgentGroupChatError.conflict
            }
            let started = try AgentTodoRepository.markInProgress(
                database,
                ownerUserID: ownerUserID,
                todoIDs: todos.map(\.id),
                nowUnixMs: nowUnixMs,
                preparedStatement: recordPreparedStatement
            )
            guard started == todos.count else { throw AgentGroupChatError.conflict }
            return try todos.map { todo in
                guard let delivery = deliveriesByTodoID[todo.id] else {
                    throw AgentGroupChatError.storage("Todo delivery batch result is missing")
                }
                return delivery
            }
        }
    }

    public func agentTodoScheduleState(
        ownerUserID: String,
        agentID: String
    ) throws -> LocalAgentTodoScheduleState {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        return try transaction {
            let startState = try AgentTodoRepository.startState(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                preparedStatement: recordPreparedStatement
            )
            guard startState.agentIsActive else {
                throw AgentGroupChatError.notFound
            }
            let running = try AgentTodoRepository.running(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                preparedStatement: recordPreparedStatement
            )
            // Match startNextReadyAgentTodo exactly: a Todo is only advertised as ready when
            // the executor lane can actually start it. This prevents manager cycles from being
            // trapped between `ready_todo_requires_start` and `no_ready_todo`.
            let ready = running == nil ? startState.todo : nil
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
            let startState = try AgentTodoRepository.startState(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                preparedStatement: recordPreparedStatement
            )
            guard startState.agentIsActive else {
                throw AgentGroupChatError.notFound
            }
            guard let todo = startState.todo else { return nil }

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
        let baseDeduplicationKey = "todo:\(todo.id)"
        let existing = try AgentDeliveryRepository.latestTodoDelivery(
            database,
            ownerUserID: ownerUserID,
            todoID: todo.id,
            preparedStatement: recordPreparedStatement
        )
        let deduplicationKey = if existing == nil {
            baseDeduplicationKey
        } else if existing?.status == .failed {
            existing!.deduplicationKey
        } else {
            "\(baseDeduplicationKey):attempt:\(nowUnixMs)"
        }
        return try prepareTodoDelivery(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todo: todo,
            deduplicationKey: deduplicationKey,
            existingDelivery: existing,
            nowUnixMs: nowUnixMs
        )
    }

    private func prepareTodoDelivery(
        ownerUserID: String,
        agentID: String,
        todo: LocalAgentTodo,
        deduplicationKey: String,
        existingDelivery: ProjectAgentDelivery?,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery {
        if let existing = existingDelivery {
            guard existing.triggerKind == .todo,
                  existing.targetAgentID == agentID else {
                throw AgentGroupChatError.conflict
            }
            if existing.status == .failed {
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
                guard sqlite3_changes(database) == 1 else {
                    throw AgentGroupChatError.conflict
                }
                return .init(
                    id: existing.id,
                    ownerUserID: existing.ownerUserID,
                    roomID: existing.roomID,
                    messageID: existing.messageID,
                    rootMessageID: existing.rootMessageID,
                    targetAgentID: existing.targetAgentID,
                    triggerKind: existing.triggerKind,
                    status: .pending,
                    attempt: existing.attempt,
                    hopCount: existing.hopCount,
                    deduplicationKey: existing.deduplicationKey,
                    createdAtUnixMs: existing.createdAtUnixMs
                )
            }
            guard existing.status == .completed || existing.status == .cancelled else {
                throw AgentGroupChatError.conflict
            }
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
        return .init(
            id: deliveryID,
            ownerUserID: ownerUserID,
            roomID: todo.teamRoomID,
            messageID: messageID,
            rootMessageID: messageID,
            targetAgentID: agentID,
            triggerKind: .todo,
            status: .pending,
            attempt: 0,
            hopCount: 0,
            deduplicationKey: deduplicationKey,
            createdAtUnixMs: nowUnixMs
        )
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
        return try AgentTodoRepository.forDelivery(
            database,
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            requireRunning: false,
            preparedStatement: recordPreparedStatement
        )
    }

    public func runningTodoForDelivery(
        ownerUserID: String,
        deliveryID: String,
        agentID: String,
        roomID: String
    ) throws -> LocalAgentTodo? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        return try AgentTodoRepository.forDelivery(
            database,
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            requireRunning: true,
            agentID: agentID,
            roomID: roomID,
            preparedStatement: recordPreparedStatement
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
            guard let todo = try AgentTodoRepository.ready(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID,
                preparedStatement: recordPreparedStatement
            ) else { return nil }
            let key = "todo-ready:\(todo.id):\(todo.updatedAtUnixMs):\(agentID)"
            let existing = try readDelivery(
                ownerUserID: ownerUserID,
                deduplicationKey: key
            )
            return try enqueueTodoReadyNotification(
                ownerUserID: ownerUserID,
                todo: todo,
                room: room,
                deduplicationKey: key,
                existingDelivery: existing,
                nowUnixMs: nowUnixMs
            )
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
        let candidates = try AgentTodoRepository.readyDependents(
            database,
            ownerUserID: ownerUserID,
            prerequisiteTodoID: prerequisiteTodoID,
            preparedStatement: recordPreparedStatement
        )
        guard !candidates.isEmpty else { return [] }
        let candidateIDs = Set(candidates.map(\.id))
        let agentIDs = Array(Set(candidates.map(\.agentID))).sorted()
        let roomsByAgentID = try ensureHumanAgentDirectRooms(
            ownerUserID: ownerUserID,
            agentIDs: agentIDs,
            nowUnixMs: nowUnixMs
        )
        return try transaction {
            let ready = try AgentTodoRepository.readyDependents(
                database,
                ownerUserID: ownerUserID,
                prerequisiteTodoID: prerequisiteTodoID,
                preparedStatement: recordPreparedStatement
            ).filter { candidateIDs.contains($0.id) }
            let keysByTodoID = Dictionary(uniqueKeysWithValues: ready.map { todo in
                (todo.id, "todo-ready:\(todo.id):\(todo.updatedAtUnixMs):\(todo.agentID)")
            })
            let existingDeliveries = try AgentDeliveryRepository.deliveries(
                database,
                ownerUserID: ownerUserID,
                deduplicationKeys: Array(keysByTodoID.values),
                preparedStatement: recordPreparedStatement
            )
            let existingByKey = Dictionary(
                uniqueKeysWithValues: existingDeliveries.map { ($0.deduplicationKey, $0) }
            )
            var newMessages: [ProjectAgentMessage] = []
            var newDeliveries: [ProjectAgentDelivery] = []
            var eventRecipients: [AgentTodoRepository.EventRecipient] = []
            var deliveriesByTodoID: [String: ProjectAgentDelivery] = [:]
            for todo in ready {
                guard let room = roomsByAgentID[todo.agentID],
                      let key = keysByTodoID[todo.id] else {
                    throw AgentGroupChatError.storage("Todo ready batch context is missing")
                }
                let delivery: ProjectAgentDelivery
                if let existing = existingByKey[key] {
                    delivery = existing
                } else {
                    let messageID = UUID().uuidString.lowercased()
                    let deliveryID = UUID().uuidString.lowercased()
                    newMessages.append(.init(
                        id: messageID,
                        ownerUserID: ownerUserID,
                        roomID: room.id,
                        draft: .init(
                            senderKind: .system,
                            senderID: "system",
                            content: "Todo 已可执行：\(todo.title)\n优先级：\(todo.priority)",
                            causationID: "todo_status",
                            rootMessageID: messageID
                        ),
                        rootMessageID: messageID,
                        createdAtUnixMs: nowUnixMs
                    ))
                    delivery = .init(
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
                        deduplicationKey: key,
                        createdAtUnixMs: nowUnixMs
                    )
                    newDeliveries.append(delivery)
                }
                deliveriesByTodoID[todo.id] = delivery
                eventRecipients.append(.init(
                    eventKey: "ready:\(todo.id):\(todo.updatedAtUnixMs)",
                    todoID: todo.id,
                    eventKind: "ready",
                    recipientAgentID: todo.agentID,
                    deliveryID: delivery.id,
                    messageID: delivery.messageID
                ))
            }
            try AgentMessageRepository.insert(
                database,
                messages: newMessages,
                preparedStatement: recordPreparedStatement
            )
            try AgentDeliveryRepository.insert(
                database,
                deliveries: newDeliveries,
                preparedStatement: recordPreparedStatement
            )
            try AgentTodoRepository.insertEventRecipients(
                database,
                ownerUserID: ownerUserID,
                recipients: eventRecipients,
                nowUnixMs: nowUnixMs,
                preparedStatement: recordPreparedStatement
            )
            return try ready.map { todo in
                guard let delivery = deliveriesByTodoID[todo.id] else {
                    throw AgentGroupChatError.storage("Todo ready batch result is missing")
                }
                return delivery
            }
        }
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
        let roomsByAgentID = try ensureHumanAgentDirectRooms(
            ownerUserID: ownerUserID,
            agentIDs: recipientIDs,
            nowUnixMs: nowUnixMs
        )
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
            let managerInstruction: String
            if todo.status == .blocked {
                managerInstruction = "如果你是该团队项目经理，请先读取完整进度并自行分流：可通过补充信息、拆单、重派、重试或团队协调解决的，不得转给 Human；负责人工作已完成而后续属于另一角色时，创建后续 Todo。只有权限、预算、凭据、产品方向决策或外部动作确实只能由 Human 完成时，才整理包含已尝试动作、影响与期限、2—3 个方案、推荐方案和明确请求的升级事项，问题较多时创建调研 Todo。"
            } else {
                managerInstruction = "如果你是该团队项目经理，请识别完成结果中的跨角色后续并创建独立 Todo，不要把已完成交付重新解释为阻塞。"
            }
            let content = "Todo 状态已更新：\(todo.title)\n状态：\(todo.status.rawValue)\n\(summary)\(suggestionNotice)\n\(managerInstruction)\n请调用 project_dashboard_get 核对实时事实；普通阻塞属于项目经理待协调事项，只有真正的 Human-only 决策或输入才能写入 Human 待办。"
            let eventKey = "status:\(todo.id):\(todo.status.rawValue):\(todo.updatedAtUnixMs)"
            let keysByRecipient = Dictionary(uniqueKeysWithValues: recipientIDs.map { recipientID in
                (
                    recipientID,
                    "todo-status:\(todo.id):\(todo.status.rawValue):\(todo.updatedAtUnixMs):\(recipientID)"
                )
            })
            let existingDeliveries = try AgentDeliveryRepository.deliveries(
                database,
                ownerUserID: ownerUserID,
                deduplicationKeys: Array(keysByRecipient.values),
                preparedStatement: recordPreparedStatement
            )
            let existingByKey = Dictionary(
                uniqueKeysWithValues: existingDeliveries.map { ($0.deduplicationKey, $0) }
            )
            var newMessages: [ProjectAgentMessage] = []
            var newDeliveries: [ProjectAgentDelivery] = []
            var eventRecipients: [AgentTodoRepository.EventRecipient] = []
            var deliveriesByRecipient: [String: ProjectAgentDelivery] = [:]
            for recipientID in recipientIDs {
                guard let room = roomsByAgentID[recipientID] else {
                    throw AgentGroupChatError.storage("Todo status room is missing")
                }
                guard let key = keysByRecipient[recipientID] else {
                    throw AgentGroupChatError.storage("Todo status key is missing")
                }
                let delivery: ProjectAgentDelivery
                if let existing = existingByKey[key] {
                    delivery = existing
                } else {
                    let messageID = UUID().uuidString.lowercased()
                    let deliveryID = UUID().uuidString.lowercased()
                    newMessages.append(.init(
                        id: messageID,
                        ownerUserID: ownerUserID,
                        roomID: room.id,
                        draft: .init(
                            senderKind: .system,
                            senderID: "system",
                            content: content,
                            causationID: "todo_status",
                            rootMessageID: messageID
                        ),
                        rootMessageID: messageID,
                        createdAtUnixMs: nowUnixMs
                    ))
                    delivery = .init(
                        id: deliveryID,
                        ownerUserID: ownerUserID,
                        roomID: room.id,
                        messageID: messageID,
                        rootMessageID: messageID,
                        targetAgentID: recipientID,
                        triggerKind: .todoStatus,
                        status: .pending,
                        attempt: 0,
                        hopCount: 0,
                        deduplicationKey: key,
                        createdAtUnixMs: nowUnixMs
                    )
                    newDeliveries.append(delivery)
                }
                deliveriesByRecipient[recipientID] = delivery
                eventRecipients.append(.init(
                    eventKey: eventKey,
                    todoID: todo.id,
                    eventKind: todo.status.rawValue,
                    recipientAgentID: recipientID,
                    deliveryID: delivery.id,
                    messageID: delivery.messageID
                ))
            }
            try AgentMessageRepository.insert(
                database,
                messages: newMessages,
                preparedStatement: recordPreparedStatement
            )
            try AgentDeliveryRepository.insert(
                database,
                deliveries: newDeliveries,
                preparedStatement: recordPreparedStatement
            )
            try AgentTodoRepository.insertEventRecipients(
                database,
                ownerUserID: ownerUserID,
                recipients: eventRecipients,
                nowUnixMs: nowUnixMs,
                preparedStatement: recordPreparedStatement
            )
            return try recipientIDs.map { recipientID in
                guard let delivery = deliveriesByRecipient[recipientID] else {
                    throw AgentGroupChatError.storage("Todo status batch result is missing")
                }
                return delivery
            }
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

    private func enqueueTodoReadyNotification(
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
