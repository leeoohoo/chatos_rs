import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    func todoGetContext(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        guard let delivery = try await store.delivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), let todo = try await todoBoundToDelivery(delivery),
        todo.agentID == context.agentID, todo.teamRoomID == context.roomID else {
            return Self.structuredFailure(
                code: "todo_execution_context_mismatch",
                field: "delivery",
                message: "当前执行线程没有有效的 Todo、Agent 或团队绑定。",
                retryable: false
            )
        }
        let sources = try await store.listTodoSources(
            ownerUserID: context.ownerUserID,
            todoIDs: [todo.id]
        )
        let sourceMessagesByID = try await store.messages(
            ownerUserID: context.ownerUserID,
            messageIDs: sources.map(\.messageID)
        )
        var sourceMessages: [TodoSourceMessageResponse] = []
        for source in sources {
            guard let message = sourceMessagesByID[source.messageID],
                  message.roomID == source.conversationID else { continue }
            var attachmentResponses: [AttachmentResponse] = []
            for attachment in message.attachmentItems {
                attachmentResponses.append(.init(
                    attachmentReference: await references.attachmentReference(
                        roomID: message.roomID,
                        messageID: message.id,
                        attachmentID: attachment.id
                    ),
                    name: attachment.name,
                    mimeType: attachment.mimeType,
                    size: attachment.size,
                    kind: attachment.kind.rawValue
                ))
            }
            sourceMessages.append(.init(
                relation: source.relation.rawValue,
                content: message.content,
                attachments: attachmentResponses,
                createdAtUnixMs: message.createdAtUnixMs
            ))
        }
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let names = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0.draft.name) })
        let dependencies = try await store.listTodoDependencies(
            ownerUserID: context.ownerUserID,
            todoIDs: [todo.id]
        )
        let prerequisitesByID = try await store.todos(
            ownerUserID: context.ownerUserID,
            todoIDs: dependencies.map(\.prerequisiteTodoID)
        )
        var prerequisites: [TodoDependencyResponse] = []
        for dependency in dependencies {
            guard let prerequisite = prerequisitesByID[dependency.prerequisiteTodoID],
                  prerequisite.agentID == dependency.prerequisiteAgentID else { continue }
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
        let progress = try await store.listAgentTodoProgress(
            ownerUserID: context.ownerUserID,
            agentID: todo.agentID,
            todoID: todo.id,
            limit: 50
        ).map(TodoProgressResponse.init(progress:))
        return try Self.outcome(TodoExecutionContextResponse(
            title: todo.title,
            detail: todo.detail,
            objective: todo.executionContract.objective,
            scope: todo.executionContract.scope,
            expectedOutputs: todo.executionContract.expectedOutputs,
            acceptanceCriteria: todo.executionContract.acceptanceCriteria,
            acceptanceChecks: todo.acceptanceChecks.map(TodoAcceptanceCheckResponse.init(check:)),
            constraints: todo.executionContract.constraints,
            priority: todo.priority,
            builtinCapabilities: todo.executionPlan.builtinCapabilities.map(\.rawValue),
            plugins: todo.executionPlan.plugins.map(\.displayName),
            sourceMessages: sourceMessages,
            prerequisites: prerequisites,
            teamAssets: teamAssets,
            progress: progress
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
            assetUpdateSuggestions: [],
            nowUnixMs: now()
        )
        return try Self.outcome(TodoProgressResponse(progress: progress))
    }

    func updateTodoAcceptance(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        guard let todo = try await currentExecutionTodo() else {
            return Self.structuredFailure(
                code: "todo_execution_context_mismatch",
                field: "delivery",
                message: "当前线程不是有效的 Todo 执行线程。",
                retryable: false
            )
        }
        guard let rawPosition = try Self.optionalInteger(arguments, key: "criterion_index") else {
            throw AgentGroupChatError.invalidField("criterion_index")
        }
        let position = Int(rawPosition)
        guard let status = LocalAgentTodoAcceptanceStatus(
            rawValue: try Self.requiredString(arguments, key: "status")
        ), status != .pending else {
            throw AgentGroupChatError.invalidField("status")
        }
        let evidence = try Self.requiredString(arguments, key: "evidence")
        let blockedReason = try Self.optionalString(arguments, key: "blocked_reason") ?? ""
        let requiresHuman = try Self.optionalBoolean(arguments, key: "requires_human") ?? false
        if status == .blocked && blockedReason.isEmpty {
            return Self.structuredFailure(
                code: "acceptance_block_reason_required",
                field: "blocked_reason",
                message: "blocked 验收项必须说明无法继续的具体原因。",
                retryable: true,
                nextTool: Self.todoAcceptanceUpdateToolName
            )
        }
        let timestamp = now()
        let updated = try await store.updateAgentTodoAcceptanceCheck(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            position: position,
            status: status,
            evidence: evidence,
            blockedReason: blockedReason,
            requiresHuman: requiresHuman,
            nowUnixMs: timestamp
        )
        let check = updated.acceptanceChecks[position - 1]
        _ = try await store.appendAgentTodoProgress(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            kind: .progress,
            runID: context.runID,
            stage: "acceptance.\(position).\(status.rawValue)",
            detail: status == .blocked ? blockedReason : evidence,
            assetUpdateSuggestions: [],
            nowUnixMs: timestamp
        )
        let humanBlocks = updated.acceptanceChecks.filter {
            $0.status == .blocked && $0.requiresHuman
        }
        let stillActionable = updated.acceptanceChecks.filter {
            $0.status != .passed && !($0.status == .blocked && $0.requiresHuman)
        }
        if !humanBlocks.isEmpty, stillActionable.isEmpty {
            let reason = humanBlocks.compactMap { check -> String? in
                let value = check.blockedReason.trimmingCharacters(in: .whitespacesAndNewlines)
                return value.isEmpty ? nil : "#\(check.position) \(value)"
            }.joined(separator: "；")
            return try await finishExecutionTodo(
                status: .blocked,
                progressKind: .blocked,
                detail: reason.isEmpty ? "剩余验收项需要 Human 介入。" : reason,
                blockedReason: reason.isEmpty ? "剩余验收项需要 Human 介入。" : reason,
                result: "",
                assetUpdateSuggestions: []
            )
        }
        return try Self.outcome(TodoAcceptanceCheckResponse(check: check))
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
        let suggestions = try Self.assetUpdateSuggestions(arguments)
        guard let todo = try await currentExecutionTodo() else {
            return Self.structuredFailure(
                code: "todo_execution_context_mismatch",
                field: "delivery",
                message: "当前线程不是有效的 Todo 执行线程。",
                retryable: false
            )
        }
        let notPassed = todo.acceptanceChecks.filter { $0.status != .passed }
        guard notPassed.isEmpty else {
            return Self.structuredFailure(
                code: "acceptance_checks_incomplete",
                field: "acceptance_checks",
                message: "Todo 不能完成，以下验收项尚未通过：\(Self.acceptanceCheckSummary(notPassed))。继续处理并逐项调用 todo_acceptance_update；确实需要 Human 的项目标记 blocked + requires_human=true 后，仍须完成其他独立项目。",
                retryable: true,
                nextTool: Self.todoAcceptanceUpdateToolName
            )
        }
        if let evidenceFailure = try await projectWriteCompletionEvidenceFailure() {
            return evidenceFailure
        }
        return try await finishExecutionTodo(
            status: .completed,
            progressKind: .completed,
            detail: summary,
            blockedReason: "",
            result: summary,
            assetUpdateSuggestions: suggestions
        )
    }

    /// A free-form summary is not proof that a project mutation happened. Project-write Todos
    /// must have program-owned receipts for a transactional commit and a later file readback in
    /// the same run before they can unlock dependants or be reported as completed.
    private func projectWriteCompletionEvidenceFailure() async throws -> AgentToolOutcome? {
        guard let todo = try await currentExecutionTodo(),
              todo.executionPlan.builtinCapabilities.contains(.projectWrite) else {
            return nil
        }
        let progress = try await store.listAgentTodoProgress(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            limit: 500
        ).filter { $0.runID == context.runID }
        guard let commit = progress.last(where: { $0.stage == "builtin.edit_committed" }) else {
            return Self.structuredFailure(
                code: "project_write_commit_evidence_missing",
                field: "summary",
                message: "当前运行没有事务编辑提交回执。请先调用 commit_edit_session；仅在总结里声称已写入不能完成 Todo。",
                retryable: true,
                nextTool: "commit_edit_session"
            )
        }
        guard progress.contains(where: {
            $0.stage == "builtin.file_read" && $0.sequence > commit.sequence
        }) else {
            return Self.structuredFailure(
                code: "project_write_readback_evidence_missing",
                field: "summary",
                message: "事务编辑已经提交，但当前运行缺少提交后的文件读取回执。请重新读取至少一个提交文件并核对结果。",
                retryable: true,
                nextTool: "read_file_raw"
            )
        }
        return nil
    }

    func blockTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let reason = try Self.requiredString(arguments, key: "reason")
        guard let todo = try await currentExecutionTodo() else {
            return Self.structuredFailure(
                code: "todo_execution_context_mismatch",
                field: "delivery",
                message: "当前线程不是有效的 Todo 执行线程。",
                retryable: false
            )
        }
        let humanBlocks = todo.acceptanceChecks.filter {
            $0.status == .blocked && $0.requiresHuman
        }
        let stillActionable = todo.acceptanceChecks.filter {
            $0.status != .passed && !($0.status == .blocked && $0.requiresHuman)
        }
        guard !humanBlocks.isEmpty else {
            return Self.structuredFailure(
                code: "human_acceptance_block_required",
                field: "acceptance_checks",
                message: "结束为 blocked 前，必须先用 todo_acceptance_update 把确实需要 Human 的验收项标记为 blocked 且 requires_human=true。",
                retryable: true,
                nextTool: Self.todoAcceptanceUpdateToolName
            )
        }
        guard stillActionable.isEmpty else {
            return Self.structuredFailure(
                code: "actionable_acceptance_checks_remaining",
                field: "acceptance_checks",
                message: "存在 Human 阻塞项，但以下独立项目仍须继续处理，不能一起跳过：\(Self.acceptanceCheckSummary(stillActionable))。",
                retryable: true,
                nextTool: Self.todoAcceptanceUpdateToolName
            )
        }
        return try await finishExecutionTodo(
            status: .blocked,
            progressKind: .blocked,
            detail: reason,
            blockedReason: reason,
            result: "",
            assetUpdateSuggestions: []
        )
    }

    static func acceptanceCheckSummary(
        _ checks: [LocalAgentTodoAcceptanceCheck]
    ) -> String {
        checks.map {
            "#\($0.position) [\($0.status.rawValue)] \($0.criterion)"
        }.joined(separator: "；")
    }

    func finishExecutionTodo(
        status: LocalAgentTodoStatus,
        progressKind: LocalAgentTodoProgressKind,
        detail: String,
        blockedReason: String,
        result: String,
        assetUpdateSuggestions: [LocalAgentTeamAssetUpdateSuggestion]
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
            assetUpdateSuggestions: assetUpdateSuggestions,
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

    static func assetUpdateSuggestions(
        _ arguments: [String: Any]
    ) throws -> [LocalAgentTeamAssetUpdateSuggestion] {
        let objects = try optionalObjectArray(arguments, key: "asset_update_suggestions")
        guard objects.count <= 8 else {
            throw AgentGroupChatError.invalidField("asset_update_suggestions")
        }
        var suggestions: [LocalAgentTeamAssetUpdateSuggestion] = []
        var identities: Set<String> = []
        var totalMarkdownBytes = 0
        for (index, object) in objects.enumerated() {
            guard let category = LocalAgentTeamAssetCategory(
                rawValue: try requiredString(object, key: "category")
            ) else {
                throw AgentGroupChatError.invalidField(
                    "asset_update_suggestions[\(index)].category"
                )
            }
            let suggestion = LocalAgentTeamAssetUpdateSuggestion(
                category: category,
                title: try requiredString(object, key: "title"),
                markdown: try requiredString(object, key: "markdown"),
                rationale: try requiredString(object, key: "rationale")
            )
            try suggestion.validate()
            let identity = "\(category.rawValue)\u{0}\(suggestion.title)"
            guard identities.insert(identity).inserted else {
                throw AgentGroupChatError.invalidField("asset_update_suggestions")
            }
            totalMarkdownBytes += suggestion.markdown.utf8.count
            guard totalMarkdownBytes <= 256_000 else {
                throw AgentGroupChatError.invalidField("asset_update_suggestions")
            }
            suggestions.append(suggestion)
        }
        return suggestions
    }

    func currentExecutionTodo() async throws -> LocalAgentTodo? {
        try await store.runningTodoForDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID,
            agentID: context.agentID,
            roomID: context.roomID
        )
    }

    func todoBoundToDelivery(_ delivery: ProjectAgentDelivery) async throws -> LocalAgentTodo? {
        guard let todoID = delivery.todoID else { return nil }
        guard let todo = try await store.todos(
            ownerUserID: context.ownerUserID,
            todoIDs: [todoID]
        )[todoID], todo.agentID == delivery.targetAgentID else { return nil }
        return todo
    }

    func todoResponses(includeTerminal: Bool) async throws -> [TodoResponse] {
        let todos = try await store.listVisibleTeamTodos(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            includeTerminal: includeTerminal
        )
        return try await todoResponses(todos)
    }

    func todoResponses(
        _ todos: [LocalAgentTodo],
        sortResponses: Bool = true
    ) async throws -> [TodoResponse] {
        guard !todos.isEmpty else { return [] }
        let rooms = try await store.listRooms(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let teamNames = Dictionary(uniqueKeysWithValues: rooms.map { ($0.id, $0.draft.name) })
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let names = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0.draft.name) })
        let todoIDs = Array(Set(todos.map(\.id))).sorted()
        var sources: [LocalAgentTodoSourceLink] = []
        var dependencies: [LocalAgentTodoDependency] = []
        for start in stride(from: 0, to: todoIDs.count, by: 500) {
            let batch = Array(todoIDs[start..<min(start + 500, todoIDs.count)])
            sources.append(contentsOf: try await store.listTodoSources(
                ownerUserID: context.ownerUserID,
                todoIDs: batch
            ))
            dependencies.append(contentsOf: try await store.listTodoDependencies(
                ownerUserID: context.ownerUserID,
                todoIDs: batch
            ))
        }
        let sourcesByTodoID = Dictionary(grouping: sources, by: \.todoID)
        let dependenciesByTodoID = Dictionary(grouping: dependencies, by: \.todoID)
        let prerequisiteIDs = Array(Set(dependencies.map(\.prerequisiteTodoID))).sorted()
        var prerequisitesByID: [String: LocalAgentTodo] = [:]
        for start in stride(from: 0, to: prerequisiteIDs.count, by: 500) {
            let batch = Array(prerequisiteIDs[start..<min(start + 500, prerequisiteIDs.count)])
            prerequisitesByID.merge(try await store.todos(
                ownerUserID: context.ownerUserID,
                todoIDs: batch
            )) { current, _ in current }
        }
        var response: [TodoResponse] = []
        response.reserveCapacity(todos.count)
        for todo in todos {
            response.append(await todoResponse(
                todo,
                teamName: teamNames[todo.teamRoomID] ?? "Team",
                names: names,
                sources: sourcesByTodoID[todo.id] ?? [],
                dependencies: dependenciesByTodoID[todo.id] ?? [],
                prerequisitesByID: prerequisitesByID
            ))
        }
        if sortResponses {
            response.sort {
                if $0.priority != $1.priority { return $0.priority > $1.priority }
                return $0.updatedAtUnixMs > $1.updatedAtUnixMs
            }
        }
        return response
    }

    func todoResponse(_ todo: LocalAgentTodo) async throws -> TodoResponse {
        guard let response = try await todoResponses([todo]).first else {
            throw AgentGroupChatError.notFound
        }
        return response
    }

    func todoResponse(
        _ todo: LocalAgentTodo,
        teamName: String,
        names: [String: String],
        sources: [LocalAgentTodoSourceLink],
        dependencies: [LocalAgentTodoDependency],
        prerequisitesByID: [String: LocalAgentTodo]
    ) async -> TodoResponse {
        let todoReference = await references.todoReference(
            todoID: todo.id,
            agentID: todo.agentID,
            teamRoomID: todo.teamRoomID
        )
        var sourceReferences: [TodoSourceReferenceResponse] = []
        for source in sources {
            sourceReferences.append(.init(
                messageReference: await references.messageReference(
                    roomID: source.conversationID,
                    messageID: source.messageID
                ),
                relation: source.relation.rawValue
            ))
        }
        var dependencyResponses: [TodoDependencyResponse] = []
        for dependency in dependencies {
            guard let prerequisite = prerequisitesByID[dependency.prerequisiteTodoID],
                  prerequisite.agentID == dependency.prerequisiteAgentID else { continue }
            dependencyResponses.append(.init(
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
            team: teamName,
            assignee: names[todo.agentID] ?? "Agent",
            assignedToCurrentAgent: todo.agentID == context.agentID,
            title: todo.title,
            detail: todo.detail,
            objective: todo.executionContract.objective,
            scope: todo.executionContract.scope,
            expectedOutputs: todo.executionContract.expectedOutputs,
            acceptanceCriteria: todo.executionContract.acceptanceCriteria,
            acceptanceChecks: todo.acceptanceChecks.map(TodoAcceptanceCheckResponse.init(check:)),
            constraints: todo.executionContract.constraints,
            priority: todo.priority,
            status: todo.status.rawValue,
            blockedReason: todo.blockedReason,
            result: todo.result,
            builtinCapabilities: todo.executionPlan.builtinCapabilities.map(\.rawValue),
            plugins: todo.executionPlan.plugins.map(\.displayName),
            sources: sourceReferences,
            dependencies: dependencyResponses,
            updatedAtUnixMs: todo.updatedAtUnixMs
        )
    }
}
