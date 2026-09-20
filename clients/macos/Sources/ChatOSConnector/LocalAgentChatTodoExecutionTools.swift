import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    func todoGetContext(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        guard let todo = try await store.todoForDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), todo.agentID == context.agentID, todo.teamRoomID == context.roomID else {
            return Self.structuredFailure(
                code: "todo_execution_context_mismatch",
                field: "delivery",
                message: "当前执行线程没有有效的 Todo、Agent 或团队绑定。",
                retryable: false
            )
        }
        let sources = try await store.listAgentTodoSources(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id
        )
        var sourceMessages: [TodoSourceMessageResponse] = []
        for source in sources {
            guard let message = try await store.message(
                ownerUserID: context.ownerUserID,
                roomID: source.conversationID,
                messageID: source.messageID
            ) else { continue }
            sourceMessages.append(.init(
                relation: source.relation.rawValue,
                content: message.content,
                attachmentNames: message.attachmentItems.map(\.name),
                createdAtUnixMs: message.createdAtUnixMs
            ))
        }
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let names = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0.draft.name) })
        var prerequisites: [TodoDependencyResponse] = []
        for dependency in try await store.listAgentTodoDependencies(
            ownerUserID: context.ownerUserID,
            agentID: todo.agentID,
            todoID: todo.id
        ) {
            guard let prerequisite = try await store.agentTodo(
                ownerUserID: context.ownerUserID,
                agentID: dependency.prerequisiteAgentID,
                todoID: dependency.prerequisiteTodoID
            ) else { continue }
            prerequisites.append(.init(
                todoReference: await references.todoReference(
                    todoID: prerequisite.id,
                    agentID: prerequisite.agentID,
                    teamRoomID: prerequisite.teamRoomID
                ),
                title: prerequisite.title,
                assignee: names[prerequisite.agentID] ?? "Agent",
                status: prerequisite.status.rawValue,
                blockedReason: prerequisite.blockedReason,
                result: prerequisite.result
            ))
        }
        var teamAssets: [TeamAssetSummaryResponse] = []
        for asset in try await store.listTodoTeamAssetSnapshots(
            ownerUserID: context.ownerUserID,
            todoID: todo.id
        ) {
            teamAssets.append(.init(
                assetReference: await references.teamAssetReference(
                    assetID: asset.assetID,
                    teamRoomID: asset.teamRoomID,
                    revision: asset.revision
                ),
                category: asset.category.rawValue,
                title: asset.title,
                revision: asset.revision,
                updatedAtUnixMs: asset.capturedAtUnixMs
            ))
        }
        return try Self.outcome(TodoExecutionContextResponse(
            title: todo.title,
            detail: todo.detail,
            objective: todo.executionContract.objective,
            scope: todo.executionContract.scope,
            expectedOutputs: todo.executionContract.expectedOutputs,
            acceptanceCriteria: todo.executionContract.acceptanceCriteria,
            constraints: todo.executionContract.constraints,
            priority: todo.priority,
            builtinCapabilities: todo.executionPlan.builtinCapabilities.map(\.rawValue),
            plugins: todo.executionPlan.plugins.map(\.displayName),
            sourceMessages: sourceMessages,
            prerequisites: prerequisites,
            teamAssets: teamAssets
        ))
    }

    func appendTodoProgress(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        guard let todo = try await currentExecutionTodo() else {
            return Self.structuredFailure(
                code: "todo_execution_context_mismatch",
                field: "delivery",
                message: "当前线程不是有效的 Todo 执行线程。",
                retryable: false
            )
        }
        let progress = try await store.appendAgentTodoProgress(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            kind: .progress,
            runID: context.runID,
            stage: try Self.optionalString(arguments, key: "stage") ?? "",
            detail: try Self.requiredString(arguments, key: "detail"),
            nowUnixMs: now()
        )
        return try Self.outcome(TodoProgressResponse(progress: progress))
    }

    func readTodoProgress(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let reference = try Self.requiredString(arguments, key: "todo_ref")
        guard let authority = await references.todoAuthority(reference: reference),
              try await isTeamMember(teamRoomID: authority.teamRoomID) else {
            return Self.structuredFailure(
                code: "invalid_todo_ref",
                field: "todo_ref",
                message: "Todo 引用无效或已经过期，请重新调用 todo_list。",
                retryable: true,
                nextTool: Self.todoListToolName
            )
        }
        let limit = Int(try Self.optionalInteger(arguments, key: "limit") ?? 100)
        let progress = try await store.listAgentTodoProgress(
            ownerUserID: context.ownerUserID,
            agentID: authority.agentID,
            todoID: authority.todoID,
            limit: limit
        )
        return try Self.outcome(progress.map(TodoProgressResponse.init(progress:)))
    }

    func completeTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let summary = try Self.requiredString(arguments, key: "summary")
        return try await finishExecutionTodo(
            status: .completed,
            progressKind: .completed,
            detail: summary,
            blockedReason: "",
            result: summary
        )
    }

    func blockTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let reason = try Self.requiredString(arguments, key: "reason")
        return try await finishExecutionTodo(
            status: .blocked,
            progressKind: .blocked,
            detail: reason,
            blockedReason: reason,
            result: ""
        )
    }

    func finishExecutionTodo(
        status: LocalAgentTodoStatus,
        progressKind: LocalAgentTodoProgressKind,
        detail: String,
        blockedReason: String,
        result: String
    ) async throws -> AgentToolOutcome {
        guard let todo = try await currentExecutionTodo() else {
            return Self.structuredFailure(
                code: "todo_execution_context_mismatch",
                field: "delivery",
                message: "当前线程不是有效的 Todo 执行线程。",
                retryable: false
            )
        }
        let timestamp = now()
        _ = try await store.appendAgentTodoProgress(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            kind: progressKind,
            runID: context.runID,
            stage: status == .completed ? "completed" : "blocked",
            detail: detail,
            nowUnixMs: timestamp
        )
        let updated = try await store.updateAgentTodo(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            update: .init(
                status: status,
                blockedReason: blockedReason,
                result: result
            ),
            nowUnixMs: timestamp
        )
        _ = try await store.completeHeartbeatDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID,
            nowUnixMs: timestamp
        )
        _ = try await store.enqueueAgentTodoStatus(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            excludingAgentID: nil,
            nowUnixMs: timestamp
        )
        if status == .completed {
            _ = try await store.enqueueReadyDependentAgentTodos(
                ownerUserID: context.ownerUserID,
                prerequisiteTodoID: todo.id,
                nowUnixMs: timestamp
            )
        }
        return try Self.outcome(try await todoResponse(updated))
    }

    func currentExecutionTodo() async throws -> LocalAgentTodo? {
        guard let delivery = try await store.delivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), delivery.status == .running, delivery.lane == .executor,
        let todo = try await store.todoForDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), todo.agentID == context.agentID, todo.status == .inProgress,
        todo.teamRoomID == context.roomID else { return nil }
        return todo
    }

    func todoResponses(includeTerminal: Bool) async throws -> [TodoResponse] {
        let rooms = try await store.listRooms(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        var response: [TodoResponse] = []
        for room in rooms where room.conversationKind == .projectTeam {
            guard try await isTeamMember(teamRoomID: room.id) else { continue }
            for todo in try await store.listTeamTodos(
                ownerUserID: context.ownerUserID,
                teamRoomID: room.id,
                includeTerminal: includeTerminal
            ) {
                response.append(try await todoResponse(todo))
            }
        }
        response.sort {
            if $0.priority != $1.priority { return $0.priority > $1.priority }
            return $0.updatedAtUnixMs > $1.updatedAtUnixMs
        }
        return response
    }

    func todoResponse(_ todo: LocalAgentTodo) async throws -> TodoResponse {
        let todoReference = await references.todoReference(
            todoID: todo.id,
            agentID: todo.agentID,
            teamRoomID: todo.teamRoomID
        )
        let team = try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: todo.teamRoomID
        )
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let names = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0.draft.name) })
        var sourceReferences: [TodoSourceReferenceResponse] = []
        for source in try await store.listAgentTodoSources(
            ownerUserID: context.ownerUserID,
            agentID: todo.agentID,
            todoID: todo.id
        ) {
            sourceReferences.append(.init(
                conversationReference: await references.conversationReference(
                    roomID: source.conversationID
                ),
                messageReference: await references.messageReference(
                    roomID: source.conversationID,
                    messageID: source.messageID
                ),
                relation: source.relation.rawValue
            ))
        }
        var dependencies: [TodoDependencyResponse] = []
        for dependency in try await store.listAgentTodoDependencies(
            ownerUserID: context.ownerUserID,
            agentID: todo.agentID,
            todoID: todo.id
        ) {
            guard let prerequisite = try await store.agentTodo(
                ownerUserID: context.ownerUserID,
                agentID: dependency.prerequisiteAgentID,
                todoID: dependency.prerequisiteTodoID
            ) else { continue }
            dependencies.append(.init(
                todoReference: await references.todoReference(
                    todoID: prerequisite.id,
                    agentID: prerequisite.agentID,
                    teamRoomID: prerequisite.teamRoomID
                ),
                title: prerequisite.title,
                assignee: names[prerequisite.agentID] ?? "Agent",
                status: prerequisite.status.rawValue,
                blockedReason: prerequisite.blockedReason,
                result: prerequisite.result
            ))
        }
        return .init(
            todoReference: todoReference,
            team: team?.draft.name ?? "Team",
            assignee: names[todo.agentID] ?? "Agent",
            assignedToCurrentAgent: todo.agentID == context.agentID,
            title: todo.title,
            detail: todo.detail,
            objective: todo.executionContract.objective,
            scope: todo.executionContract.scope,
            expectedOutputs: todo.executionContract.expectedOutputs,
            acceptanceCriteria: todo.executionContract.acceptanceCriteria,
            constraints: todo.executionContract.constraints,
            priority: todo.priority,
            status: todo.status.rawValue,
            blockedReason: todo.blockedReason,
            result: todo.result,
            builtinCapabilities: todo.executionPlan.builtinCapabilities.map(\.rawValue),
            plugins: todo.executionPlan.plugins.map(\.displayName),
            sources: sourceReferences,
            dependencies: dependencies,
            updatedAtUnixMs: todo.updatedAtUnixMs
        )
    }
}
