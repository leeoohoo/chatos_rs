import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    func updateTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let todoReference = try Self.requiredString(arguments, key: "todo_ref")
        guard let authority = await references.todoAuthority(reference: todoReference) else {
            return Self.structuredFailure(
                code: "invalid_todo_ref",
                field: "todo_ref",
                message: "Todo 引用无效或已经过期，请重新调用 todo_list。",
                retryable: true,
                nextTool: Self.todoListToolName
            )
        }
        guard try await isProjectManager(teamRoomID: authority.teamRoomID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "todo_ref",
                message: "只有该团队明确指定的项目经理可以修改团队任务。",
                retryable: false
            )
        }
        guard let existingTodo = try await store.agentTodo(
            ownerUserID: context.ownerUserID,
            agentID: authority.agentID,
            todoID: authority.todoID
        ) else { throw AgentGroupChatError.notFound }
        let status = try Self.optionalString(arguments, key: "status").flatMap(
            LocalAgentTodoStatus.init(rawValue:)
        )
        if arguments["status"] != nil, status == nil {
            return Self.structuredFailure(
                code: "invalid_todo_status",
                field: "status",
                message: "通讯线程只能把 Todo 重新置为 pending，或将它取消。",
                retryable: true
            )
        }
        if let status, status != .pending && status != .cancelled {
            return Self.structuredFailure(
                code: "executor_owned_status",
                field: "status",
                message: "completed 和 blocked 状态由独立 Todo 执行线程写入，通讯线程不能代替执行。",
                retryable: true
            )
        }
        let sourceReferences = try Self.optionalStringArray(
            arguments,
            key: "source_message_refs"
        )
        var linkedSources: [LocalAgentTodoSourceDraft] = []
        let relation: LocalAgentTodoSourceRelation = arguments["priority"] == nil
            ? .updated
            : .reprioritized
        for reference in sourceReferences {
            guard let source = await references.messageAuthority(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_source_message_ref",
                    field: "source_message_refs",
                    message: "补充来源消息引用不是当前 Run 的工具签发值、已损坏，或不属于当前 Agent。若来源是本次唤醒消息，请重新调用 chat_get_trigger；若来源是新收件箱消息，请调用 chat_read_all_unread。",
                    retryable: true,
                    nextTool: Self.getTriggerToolName
                )
            }
            linkedSources.append(.init(
                roomID: source.roomID,
                messageID: source.messageID,
                relation: relation
            ))
        }
        var dependencyDrafts: [LocalAgentTodoDependencyDraft]?
        if arguments["depends_on_todo_refs"] != nil {
            let dependencyReferences = try Self.optionalStringArray(
                arguments,
                key: "depends_on_todo_refs"
            )
            var parsed: [LocalAgentTodoDependencyDraft] = []
            var dependencyIDs = Set<String>()
            for (index, reference) in dependencyReferences.enumerated() {
                guard let prerequisite = await references.todoAuthority(reference: reference) else {
                    return Self.structuredFailure(
                        code: "invalid_dependency_todo_ref",
                        field: "depends_on_todo_refs[\(index)]",
                        message: "前置任务引用无效或已经过期。请重新读取团队前置任务选项。",
                        retryable: true,
                        nextTool: Self.todoDependencyOptionsToolName
                    )
                }
                guard prerequisite.teamRoomID == authority.teamRoomID else {
                    return Self.structuredFailure(
                        code: "cross_team_dependency",
                        field: "depends_on_todo_refs[\(index)]",
                        message: "前置任务必须与当前任务属于同一个项目团队。",
                        retryable: true,
                        nextTool: Self.todoDependencyOptionsToolName
                    )
                }
                guard prerequisite.todoID != authority.todoID else {
                    return Self.structuredFailure(
                        code: "self_dependency",
                        field: "depends_on_todo_refs[\(index)]",
                        message: "任务不能依赖自己。",
                        retryable: true
                    )
                }
                guard dependencyIDs.insert(prerequisite.todoID).inserted else {
                    return Self.structuredFailure(
                        code: "duplicate_dependency",
                        field: "depends_on_todo_refs",
                        message: "同一个前置任务不能重复添加。",
                        retryable: true
                    )
                }
                parsed.append(.init(
                    prerequisiteTodoID: prerequisite.todoID,
                    prerequisiteAgentID: prerequisite.agentID
                ))
            }
            dependencyDrafts = parsed
        }
        let timestamp = now()
        let contractKeys = [
            "objective", "scope", "expected_outputs", "acceptance_criteria", "constraints",
        ]
        let executionContract: LocalAgentTodoExecutionContract?
        if contractKeys.contains(where: { arguments[$0] != nil }) {
            executionContract = .init(
                objective: try Self.optionalString(arguments, key: "objective")
                    ?? existingTodo.executionContract.objective,
                scope: try Self.optionalString(arguments, key: "scope")
                    ?? existingTodo.executionContract.scope,
                expectedOutputs: arguments["expected_outputs"] == nil
                    ? existingTodo.executionContract.expectedOutputs
                    : try Self.optionalStringArray(arguments, key: "expected_outputs"),
                acceptanceCriteria: arguments["acceptance_criteria"] == nil
                    ? existingTodo.executionContract.acceptanceCriteria
                    : try Self.optionalStringArray(arguments, key: "acceptance_criteria"),
                constraints: arguments["constraints"] == nil
                    ? existingTodo.executionContract.constraints
                    : try Self.optionalStringArray(arguments, key: "constraints")
            )
        } else {
            executionContract = nil
        }
        let todo = try await store.updateAgentTodo(
            ownerUserID: context.ownerUserID,
            agentID: authority.agentID,
            todoID: authority.todoID,
            update: .init(
                title: try Self.optionalString(arguments, key: "title"),
                detail: try Self.optionalString(arguments, key: "detail"),
                priority: try Self.optionalInteger(arguments, key: "priority").map(Int.init),
                status: status,
                blockedReason: status == .pending ? "" : nil,
                result: nil,
                executionContract: executionContract
            ),
            nowUnixMs: timestamp
        )
        if status == .cancelled, existingTodo.status != .cancelled {
            await todoCancellationHandler(todo.id)
            _ = try await store.appendAgentTodoProgress(
                ownerUserID: context.ownerUserID,
                agentID: authority.agentID,
                todoID: authority.todoID,
                kind: .cancelled,
                runID: context.runID,
                stage: "cancelled",
                detail: "项目经理已停止该任务。",
                assetUpdateSuggestions: [],
                nowUnixMs: timestamp
            )
            _ = try await store.enqueueAgentTodoStatus(
                ownerUserID: context.ownerUserID,
                agentID: authority.agentID,
                todoID: authority.todoID,
                excludingAgentID: context.agentID,
                nowUnixMs: timestamp
            )
        }
        if !linkedSources.isEmpty {
            _ = try await store.linkAgentTodoSources(
                ownerUserID: context.ownerUserID,
                agentID: authority.agentID,
                todoID: authority.todoID,
                sources: linkedSources,
                nowUnixMs: timestamp
            )
        }
        if let dependencyDrafts {
            do {
                _ = try await store.setAgentTodoDependencies(
                    ownerUserID: context.ownerUserID,
                    agentID: authority.agentID,
                    todoID: authority.todoID,
                    dependencies: dependencyDrafts,
                    nowUnixMs: timestamp
                )
            } catch let error as AgentGroupChatError {
                return Self.structuredFailure(
                    code: error == .invalidField("todoDependencyCycle")
                        ? "dependency_cycle" : Self.errorCode(error),
                    field: Self.errorField(error) ?? "depends_on_todo_refs",
                    message: error == .invalidField("todoDependencyCycle")
                        ? "这些前置关系会形成依赖环，请重新拆分或调整依赖。"
                        : error.localizedDescription,
                    retryable: true,
                    nextTool: Self.todoDependencyOptionsToolName
                )
            }
        }
        if todo.status == .pending, authority.agentID != context.agentID {
            _ = try await store.enqueueAgentTodoReady(
                ownerUserID: context.ownerUserID,
                agentID: authority.agentID,
                todoID: authority.todoID,
                nowUnixMs: timestamp
            )
        }
        return try Self.outcome(try await todoResponse(todo))
    }

}
