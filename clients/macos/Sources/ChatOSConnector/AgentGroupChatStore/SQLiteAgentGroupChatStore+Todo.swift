import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func listAgentTodos(
        ownerUserID: String,
        agentID: String,
        includeTerminal: Bool = false
    ) throws -> [LocalAgentTodo] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        return try AgentTodoRepository.listForAgent(
            database,
            ownerUserID: ownerUserID,
            agentID: agentID,
            includeTerminal: includeTerminal,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listTeamTodos(
        ownerUserID: String,
        teamRoomID: String,
        includeTerminal: Bool = false
    ) throws -> [LocalAgentTodo] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: teamRoomID)?.conversationKind
            == .projectTeam else {
            throw AgentGroupChatError.notFound
        }
        return try AgentTodoRepository.listForTeam(
            database,
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            includeTerminal: includeTerminal,
            preparedStatement: recordPreparedStatement
        )
    }

    public func createAgentTodo(
        ownerUserID: String,
        agentID: String,
        requestKey: String,
        draft: LocalAgentTodoDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentTodo {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try AgentGroupChatValidation.text(draft.title, field: "todoTitle", maximumLength: 500)
        try AgentGroupChatValidation.optionalText(draft.detail, field: "todoDetail", maximumLength: 16_000)
        try draft.executionPlan.validate()
        let executionContract = draft.executionContract.normalized(
            title: draft.title,
            detail: draft.detail
        )
        try executionContract.validate()
        guard (0...100).contains(draft.priority), nowUnixMs >= 0,
              draft.dependencies.count <= 64,
              Set(draft.dependencies.map(\.prerequisiteTodoID)).count == draft.dependencies.count else {
            throw AgentGroupChatError.invalidField("todoPriority")
        }
        for dependency in draft.dependencies {
            try AgentGroupChatValidation.identifier(
                dependency.prerequisiteTodoID,
                field: "todoDependency"
            )
            try AgentGroupChatValidation.identifier(
                dependency.prerequisiteAgentID,
                field: "todoDependencyAgent"
            )
        }
        return try transaction {
            if let existing = try AgentTodoRepository.find(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                requestKey: requestKey,
                preparedStatement: recordPreparedStatement
            ) { return existing }
            guard try readAgent(ownerUserID: ownerUserID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let creatorAgentID = draft.creatorAgentID ?? agentID
            guard try readAgent(
                ownerUserID: ownerUserID,
                agentID: creatorAgentID
            )?.status == .active else { throw AgentGroupChatError.notFound }
            if let roomID = draft.sourceRoomID {
                guard try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: creatorAgentID
                )?.status == .active else { throw AgentGroupChatError.notMember }
                if let messageID = draft.sourceMessageID {
                    guard try readMessage(ownerUserID: ownerUserID, messageID: messageID)?.roomID == roomID else {
                        throw AgentGroupChatError.notFound
                    }
                }
            } else if draft.sourceMessageID != nil {
                throw AgentGroupChatError.invalidField("sourceMessageID")
            }
            for source in draft.additionalSources {
                try AgentGroupChatValidation.identifier(source.roomID, field: "sourceConversation")
                try AgentGroupChatValidation.identifier(source.messageID, field: "sourceMessage")
                guard try readMember(
                    ownerUserID: ownerUserID,
                    roomID: source.roomID,
                    agentID: creatorAgentID
                )?.status == .active,
                try readMessage(
                    ownerUserID: ownerUserID,
                    messageID: source.messageID
                )?.roomID == source.roomID else {
                    throw AgentGroupChatError.invalidField("sourceMessageRefs")
                }
            }
            let resolvedTeamRoomID: String
            if let requested = draft.teamRoomID {
                resolvedTeamRoomID = requested
            } else if let sourceRoomID = draft.sourceRoomID,
                      try readRoom(
                        ownerUserID: ownerUserID,
                        roomID: sourceRoomID
                      )?.conversationKind == .projectTeam {
                // Compatibility for trusted native callers. Model-facing tools always require a
                // team_ref and never infer the execution boundary from arbitrary message text.
                resolvedTeamRoomID = sourceRoomID
            } else {
                throw AgentGroupChatError.invalidField("teamRef")
            }
            guard let team = try readRoom(
                ownerUserID: ownerUserID,
                roomID: resolvedTeamRoomID
            ), team.status == .active, team.conversationKind == .projectTeam,
               (draft.creatorAgentID == nil
                   || team.projectManagerAgentID == creatorAgentID),
               try readMember(
                   ownerUserID: ownerUserID,
                   roomID: resolvedTeamRoomID,
                   agentID: agentID
               )?.status == .active else {
                throw AgentGroupChatError.invalidField("teamRef")
            }
            let nextOrder = try AgentTodoRepository.nextSortOrder(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                preparedStatement: recordPreparedStatement
            )
            let todo = LocalAgentTodo(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                agentID: agentID,
                teamRoomID: resolvedTeamRoomID,
                sourceRoomID: draft.sourceRoomID,
                sourceMessageID: draft.sourceMessageID,
                title: draft.title,
                detail: draft.detail,
                priority: draft.priority,
                sortOrder: nextOrder,
                executionContract: executionContract,
                executionPlan: draft.executionPlan,
                createdAtUnixMs: nowUnixMs,
                updatedAtUnixMs: nowUnixMs
            )
            try todo.validate()
            try execute(
                """
                INSERT INTO local_agent_todos (
                    owner_user_id, id, agent_id, team_room_id, source_room_id, source_message_id,
                    request_key, title, detail, priority, sort_order, status,
                    blocked_reason, result, created_at_unix_ms, updated_at_unix_ms,
                    execution_plan_json, execution_contract_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', '', '', ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(todo.id), .text(agentID),
                    .text(resolvedTeamRoomID),
                    draft.sourceRoomID.map(Value.text) ?? .null,
                    draft.sourceMessageID.map(Value.text) ?? .null,
                    .text(requestKey), .text(draft.title), .text(draft.detail),
                    .integer(Int64(draft.priority)), .integer(nextOrder),
                    .integer(nowUnixMs), .integer(nowUnixMs),
                    .text(try encodeJSON(draft.executionPlan)),
                    .text(try encodeJSON(executionContract)),
                ]
            )
            var sources = draft.additionalSources
            if let roomID = draft.sourceRoomID, let messageID = draft.sourceMessageID {
                sources.insert(.init(roomID: roomID, messageID: messageID), at: 0)
            }
            var inserted = Set<String>()
            for source in sources {
                let key = "\(source.roomID)\u{0}\(source.messageID)\u{0}\(source.relation.rawValue)"
                guard inserted.insert(key).inserted else { continue }
                try execute(
                    """
                    INSERT INTO local_agent_todo_sources (
                        owner_user_id, todo_id, conversation_id, message_id, relation,
                        created_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, ?)
                    """,
                    [
                        .text(ownerUserID), .text(todo.id), .text(source.roomID),
                        .text(source.messageID), .text(source.relation.rawValue),
                        .integer(nowUnixMs),
                    ]
                )
            }
            _ = try replaceAgentTodoDependencies(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todo.id,
                dependencies: draft.dependencies,
                nowUnixMs: nowUnixMs
            )
            return todo
        }
    }

    public func updateAgentTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        update: LocalAgentTodoUpdate,
        nowUnixMs: Int64
    ) throws -> LocalAgentTodo {
        try updateAgentTodo(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID,
            update: update,
            nowUnixMs: nowUnixMs,
            allowsInterruptedReviewRetry: false
        )
    }

    public func agentTodoRequiresHumanRetry(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) throws -> Bool {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        return try hasInterruptedRunRequiringHumanRetry(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID
        )
    }

    /// The scheduler calls this only after an explicit Human retry has cleared the Run's
    /// `needsReview` checkpoint in memory. Ordinary Agent tools must use `updateAgentTodo`,
    /// which cannot reopen a Todo while its durable Run still requires Human review.
    func updateAgentTodoAfterHumanReview(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        update: LocalAgentTodoUpdate,
        nowUnixMs: Int64
    ) throws -> LocalAgentTodo {
        try updateAgentTodo(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID,
            update: update,
            nowUnixMs: nowUnixMs,
            allowsInterruptedReviewRetry: true
        )
    }

    private func updateAgentTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        update: LocalAgentTodoUpdate,
        nowUnixMs: Int64,
        allowsInterruptedReviewRetry: Bool
    ) throws -> LocalAgentTodo {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let existing = try readTodo(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID
            ) else { throw AgentGroupChatError.notFound }
            let title = update.title ?? existing.title
            let detail = update.detail ?? existing.detail
            let priority = update.priority ?? existing.priority
            let status = update.status ?? existing.status
            let blockedReason = update.blockedReason ?? existing.blockedReason
            let result = update.result ?? existing.result
            let executionContract = update.executionContract ?? existing.executionContract
            if existing.status == .blocked,
               status == .pending,
               !allowsInterruptedReviewRetry {
                guard try !hasInterruptedRunRequiringHumanRetry(
                    ownerUserID: ownerUserID,
                    agentID: agentID,
                    todoID: todoID
                ) else {
                    throw AgentGroupChatError.conflict
                }
            }
            let revised = LocalAgentTodo(
                id: existing.id,
                ownerUserID: existing.ownerUserID,
                agentID: existing.agentID,
                teamRoomID: existing.teamRoomID,
                sourceRoomID: existing.sourceRoomID,
                sourceMessageID: existing.sourceMessageID,
                title: title,
                detail: detail,
                priority: priority,
                sortOrder: existing.sortOrder,
                status: status,
                blockedReason: blockedReason,
                result: result,
                executionContract: executionContract,
                executionPlan: existing.executionPlan,
                createdAtUnixMs: existing.createdAtUnixMs,
                updatedAtUnixMs: max(nowUnixMs, existing.createdAtUnixMs)
            )
            try revised.validate()
            let transitionAllowed: Bool
            if status == existing.status {
                transitionAllowed = true
            } else {
                transitionAllowed = switch (existing.status, status) {
                case (.pending, .inProgress), (.pending, .blocked), (.pending, .cancelled),
                     (.inProgress, .completed), (.inProgress, .blocked), (.inProgress, .cancelled),
                     (.blocked, .pending), (.blocked, .cancelled),
                     (.completed, .pending), (.cancelled, .pending):
                    true
                default:
                    false
                }
            }
            guard transitionAllowed else {
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE local_agent_todos
                SET title = ?, detail = ?, priority = ?, status = ?, blocked_reason = ?,
                    result = ?, execution_contract_json = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND agent_id = ? AND id = ? AND status = ?
                """,
                [
                    .text(title), .text(detail), .integer(Int64(priority)),
                    .text(status.rawValue), .text(blockedReason), .text(result),
                    .text(try encodeJSON(executionContract)), .integer(revised.updatedAtUnixMs),
                    .text(ownerUserID), .text(agentID),
                    .text(todoID), .text(existing.status.rawValue),
                ]
            )
            guard sqlite3_changes(database) == 1 else {
                throw AgentGroupChatError.conflict
            }
            if status == .cancelled, existing.status != .cancelled {
                try execute(
                    """
                    UPDATE project_agent_deliveries
                    SET status = 'cancelled', last_error = ?, completed_at_unix_ms = ?
                    WHERE owner_user_id = ? AND target_agent_id = ?
                      AND deduplication_key = ? AND trigger_kind = 'todo'
                      AND status IN ('pending', 'running')
                    """,
                    [
                        .text("Todo 已由项目经理停止。"), .integer(revised.updatedAtUnixMs),
                        .text(ownerUserID), .text(agentID), .text("todo:\(todoID)"),
                    ]
                )
            }
            return revised
        }
    }

    private func hasInterruptedRunRequiringHumanRetry(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) throws -> Bool {
        try scalarInt64(
            """
            SELECT COUNT(*)
            FROM project_agent_deliveries delivery
            JOIN local_agent_group_chat_runs run
              ON run.owner_user_id = delivery.owner_user_id
             AND run.delivery_id = delivery.id
            WHERE delivery.owner_user_id = ?
              AND delivery.target_agent_id = ?
              AND delivery.deduplication_key = ?
              AND delivery.trigger_kind = 'todo'
              AND delivery.status = 'running'
              AND run.status = ?
            """,
            [
                .text(ownerUserID), .text(agentID), .text("todo:\(todoID)"),
                .text(AgentRunCheckpoint.Status.needsReview.rawValue),
            ]
        ) > 0
    }

    public func reorderAgentTodos(
        ownerUserID: String,
        agentID: String,
        todoIDs: [String],
        nowUnixMs: Int64
    ) throws -> [LocalAgentTodo] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifiers(todoIDs, field: "todoIDs", maximumCount: 500)
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            let active = try listAgentTodos(
                ownerUserID: ownerUserID,
                agentID: agentID,
                includeTerminal: false
            )
            let byID = Dictionary(uniqueKeysWithValues: active.map { ($0.id, $0) })
            guard todoIDs.allSatisfy({ byID[$0] != nil }) else {
                throw AgentGroupChatError.notFound
            }
            let requested = Set(todoIDs)
            let ordered = todoIDs + active.map(\.id).filter { !requested.contains($0) }
            for (offset, id) in ordered.enumerated() {
                let reorderedPriority = max(0, 100 - offset)
                try execute(
                    """
                    UPDATE local_agent_todos
                    SET priority = ?, sort_order = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND agent_id = ? AND id = ?
                    """,
                    [
                        .integer(Int64(reorderedPriority)), .integer(Int64(offset)),
                        .integer(nowUnixMs), .text(ownerUserID), .text(agentID), .text(id),
                    ]
                )
            }
            return try listAgentTodos(
                ownerUserID: ownerUserID,
                agentID: agentID,
                includeTerminal: false
            )
        }
    }

    public func reorderTeamTodos(
        ownerUserID: String,
        teamRoomID: String,
        todoIDs: [String],
        nowUnixMs: Int64
    ) throws -> [LocalAgentTodo] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        try AgentGroupChatValidation.identifiers(todoIDs, field: "todoIDs", maximumCount: 500)
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            let active = try listTeamTodos(
                ownerUserID: ownerUserID,
                teamRoomID: teamRoomID,
                includeTerminal: false
            )
            let byID = Dictionary(uniqueKeysWithValues: active.map { ($0.id, $0) })
            guard todoIDs.allSatisfy({ byID[$0] != nil }) else {
                throw AgentGroupChatError.notFound
            }
            let requested = Set(todoIDs)
            let ordered = todoIDs + active.map(\.id).filter { !requested.contains($0) }
            for (offset, id) in ordered.enumerated() {
                let reorderedPriority = max(0, 100 - offset)
                try execute(
                    """
                    UPDATE local_agent_todos
                    SET priority = ?, sort_order = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND team_room_id = ? AND id = ?
                    """,
                    [
                        .integer(Int64(reorderedPriority)), .integer(Int64(offset)),
                        .integer(nowUnixMs), .text(ownerUserID), .text(teamRoomID), .text(id),
                    ]
                )
            }
            return try listTeamTodos(
                ownerUserID: ownerUserID,
                teamRoomID: teamRoomID,
                includeTerminal: false
            )
        }
    }

}
