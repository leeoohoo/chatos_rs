import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    func listTodos(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let includeTerminal = try Self.optionalBoolean(
            arguments,
            key: "include_terminal"
        ) ?? false
        return try Self.outcome(try await todoResponses(includeTerminal: includeTerminal))
    }

    func todoScheduleState(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        let state = try await store.agentTodoScheduleState(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID
        )
        let runningResponse: TodoResponse?
        if let runningTodo = state.runningTodo {
            runningResponse = try await todoResponse(runningTodo)
        } else {
            runningResponse = nil
        }
        let readyResponse: TodoResponse?
        if let readyTodo = state.readyTodo {
            readyResponse = try await todoResponse(readyTodo)
        } else {
            readyResponse = nil
        }
        return try Self.outcome(TodoScheduleStateResponse(
            state: state.runningTodo != nil ? "busy" : (state.readyTodo != nil ? "ready" : "idle"),
            runningTodo: runningResponse,
            readyTodo: readyResponse
        ))
    }

    func startNextTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        let delivery = try await store.startNextReadyAgentTodo(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            nowUnixMs: now()
        )
        if let delivery,
           let todo = try await store.todoForDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: delivery.id
           ) {
            return try Self.outcome(TodoStartNextResponse(
                status: "started",
                todo: try await todoResponse(todo)
            ))
        }
        let state = try await store.agentTodoScheduleState(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID
        )
        let runningResponse: TodoResponse?
        if let runningTodo = state.runningTodo {
            runningResponse = try await todoResponse(runningTodo)
        } else {
            runningResponse = nil
        }
        return try Self.outcome(TodoStartNextResponse(
            status: state.runningTodo != nil ? "executor_busy" : "no_ready_todo",
            todo: runningResponse
        ))
    }

    func todoExecutionOptions(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        let rooms = try await store.listRooms(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        let profilesByID = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0) })
        var teams: [TodoTeamOptionResponse] = []
        for room in rooms where room.conversationKind == .projectTeam {
            guard room.projectManagerAgentID == context.agentID else { continue }
            let members = try await store.listMembers(
                ownerUserID: context.ownerUserID,
                roomID: room.id
            )
            var assignees: [TodoAssigneeOptionResponse] = []
            for member in members where member.status == .active {
                guard let profile = profilesByID[member.agentID] else { continue }
                assignees.append(.init(
                    assigneeReference: await references.assigneeReference(
                        agentID: member.agentID,
                        teamRoomID: room.id
                    ),
                    name: profile.draft.name,
                    profession: profile.draft.professionKey,
                    role: member.draft.role,
                    isProjectManager: room.projectManagerAgentID == member.agentID
                ))
            }
            teams.append(.init(
                teamReference: await references.teamReference(teamID: room.id),
                name: room.draft.name,
                goal: room.draft.goal,
                assignees: assignees
            ))
        }
        var plugins: [TodoPluginOptionResponse] = []
        for option in todoPluginOptions {
            plugins.append(.init(
                pluginReference: await references.pluginReference(option: option),
                name: option.displayName,
                description: option.description
            ))
        }
        return try Self.outcome(TodoExecutionOptionsResponse(
            teams: teams,
            builtinCapabilities: LocalAgentTodoBuiltinCapability.allCases.map(\.rawValue),
            plugins: plugins
        ))
    }

    func todoDependencyOptions(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let teamReference = try Self.requiredString(arguments, key: "team_ref")
        guard let teamID = await references.teamID(reference: teamReference),
              try await isProjectManager(teamRoomID: teamID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "team_ref",
                message: "只有该团队明确指定的项目经理可以管理任务与前置依赖。",
                retryable: true,
                nextTool: Self.todoExecutionOptionsToolName
            )
        }
        let todos = try await store.listTeamTodos(
            ownerUserID: context.ownerUserID,
            teamRoomID: teamID,
            includeTerminal: true
        ).filter { $0.status != .cancelled }
        var response: [TodoDependencyOptionResponse] = []
        for todo in todos {
            let profile = try await store.listAgents(
                ownerUserID: context.ownerUserID,
                includeArchived: true
            ).first(where: { $0.id == todo.agentID })
            response.append(.init(
                todoReference: await references.todoReference(
                    todoID: todo.id,
                    agentID: todo.agentID,
                    teamRoomID: todo.teamRoomID
                ),
                title: todo.title,
                assignee: profile?.draft.name ?? "Agent",
                status: todo.status.rawValue,
                blockedReason: todo.blockedReason,
                result: todo.result
            ))
        }
        return try Self.outcome(response)
    }

    func addTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let teamReference = try Self.requiredString(arguments, key: "team_ref")
        guard let teamID = await references.teamID(reference: teamReference) else {
            return Self.structuredFailure(
                code: "invalid_team_ref",
                field: "team_ref",
                message: "团队选项无效或已经过期。请重新调用 todo_execution_options。",
                retryable: true,
                nextTool: Self.todoExecutionOptionsToolName
            )
        }
        guard try await isProjectManager(teamRoomID: teamID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "team_ref",
                message: "只有该团队明确指定的项目经理可以创建和分配团队任务。",
                retryable: false
            )
        }
        let assigneeID: String
        if let assigneeReference = try Self.optionalString(arguments, key: "assignee_ref") {
            guard let authority = await references.assigneeAuthority(
                reference: assigneeReference
            ), authority.teamRoomID == teamID else {
                return Self.structuredFailure(
                    code: "invalid_assignee_ref",
                    field: "assignee_ref",
                    message: "负责人选项无效、已过期或不属于所选团队。请重新读取执行选项。",
                    retryable: true,
                    nextTool: Self.todoExecutionOptionsToolName
                )
            }
            assigneeID = authority.agentID
        } else {
            assigneeID = context.agentID
        }
        let dependencyReferences = try Self.optionalStringArray(
            arguments,
            key: "depends_on_todo_refs"
        )
        var dependencies: [LocalAgentTodoDependencyDraft] = []
        var dependencyIDs = Set<String>()
        for (index, reference) in dependencyReferences.enumerated() {
            guard let authority = await references.todoAuthority(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_dependency_todo_ref",
                    field: "depends_on_todo_refs[\(index)]",
                    message: "前置任务引用无效或已经过期。请重新读取团队前置任务选项。",
                    retryable: true,
                    nextTool: Self.todoDependencyOptionsToolName
                )
            }
            guard authority.teamRoomID == teamID else {
                return Self.structuredFailure(
                    code: "cross_team_dependency",
                    field: "depends_on_todo_refs[\(index)]",
                    message: "前置任务必须与当前任务属于同一个项目团队。",
                    retryable: true,
                    nextTool: Self.todoDependencyOptionsToolName
                )
            }
            guard dependencyIDs.insert(authority.todoID).inserted else {
                return Self.structuredFailure(
                    code: "duplicate_dependency",
                    field: "depends_on_todo_refs",
                    message: "同一个前置任务不能重复添加。",
                    retryable: true
                )
            }
            dependencies.append(.init(
                prerequisiteTodoID: authority.todoID,
                prerequisiteAgentID: authority.agentID
            ))
        }
        let sourceReferences = try Self.optionalStringArray(
            arguments,
            key: "source_message_refs"
        )
        guard !sourceReferences.isEmpty else {
            return Self.structuredFailure(
                code: "missing_source_messages",
                field: "source_message_refs",
                message: "Todo 必须关联至少一条本轮已经读取的来源消息。",
                retryable: true,
                nextTool: Self.readAllUnreadToolName
            )
        }
        var sources: [LocalAgentTodoSourceDraft] = []
        for reference in sourceReferences {
            guard let source = await references.messageAuthority(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_source_message_ref",
                    field: "source_message_refs",
                    message: "来源消息引用无效、已过期，或不属于当前 Agent 的本轮收件箱。",
                    retryable: true,
                    nextTool: Self.readAllUnreadToolName
                )
            }
            sources.append(.init(roomID: source.roomID, messageID: source.messageID))
        }
        let requestedKinds = try Self.optionalStringArray(
            arguments,
            key: "builtin_capabilities"
        )
        var builtinCapabilities: [LocalAgentTodoBuiltinCapability] = []
        for rawValue in requestedKinds {
            guard let capability = LocalAgentTodoBuiltinCapability(rawValue: rawValue) else {
                return Self.structuredFailure(
                    code: "unsupported_builtin_capability",
                    field: "builtin_capabilities",
                    message: "请求了客户端未提供的基础能力。请重新读取执行选项。",
                    retryable: true,
                    nextTool: Self.todoExecutionOptionsToolName
                )
            }
            builtinCapabilities.append(capability)
        }
        guard Set(builtinCapabilities).count == builtinCapabilities.count else {
            return Self.structuredFailure(
                code: "duplicate_builtin_capability",
                field: "builtin_capabilities",
                message: "同一种基础能力不能重复选择。",
                retryable: true
            )
        }
        let requiresExecution = try Self.optionalBoolean(
            arguments,
            key: "requires_execution"
        ) ?? true
        if !requiresExecution,
           builtinCapabilities.contains(where: { $0 != .projectRead }) {
            return Self.structuredFailure(
                code: "execution_required",
                field: "requires_execution",
                message: "文件写入或终端能力需要执行环境，请将 requires_execution 设为 true。",
                retryable: true
            )
        }
        let pluginHints = try Self.optionalObjectArray(arguments, key: "plugin_hints")
        var pluginSelections: [LocalAgentTodoPluginSelection] = []
        var selectedPluginIDs = Set<String>()
        for (index, hint) in pluginHints.enumerated() {
            guard let reference = hint["plugin_ref"] as? String,
                  let option = await references.plugin(reference: reference) else {
                return Self.structuredFailure(
                    code: "plugin_not_selectable",
                    field: "plugin_hints[\(index)].plugin_ref",
                    message: "Plugin 选项无效、已停用或已经过期。请重新读取本机执行选项。",
                    retryable: true,
                    nextTool: Self.todoExecutionOptionsToolName
                )
            }
            guard selectedPluginIDs.insert(option.pluginID).inserted else {
                return Self.structuredFailure(
                    code: "duplicate_plugin",
                    field: "plugin_hints[\(index)].plugin_ref",
                    message: "同一个 Plugin 不能被重复选择。",
                    retryable: true
                )
            }
            let reason = (hint["reason"] as? String) ?? ""
            pluginSelections.append(.init(
                pluginID: option.pluginID,
                displayName: option.displayName,
                reason: reason
            ))
        }
        let expectedOutputs = try Self.optionalStringArray(arguments, key: "expected_outputs")
        let acceptanceCriteria = try Self.optionalStringArray(
            arguments,
            key: "acceptance_criteria"
        )
        guard !expectedOutputs.isEmpty, !acceptanceCriteria.isEmpty else {
            return Self.structuredFailure(
                code: "incomplete_execution_contract",
                field: expectedOutputs.isEmpty ? "expected_outputs" : "acceptance_criteria",
                message: "创建 Todo 必须明确交付物和可核验的验收条件。",
                retryable: true
            )
        }
        let executionContract = LocalAgentTodoExecutionContract(
            objective: try Self.requiredString(arguments, key: "objective"),
            scope: try Self.requiredString(arguments, key: "scope"),
            expectedOutputs: expectedOutputs,
            acceptanceCriteria: acceptanceCriteria,
            constraints: try Self.optionalStringArray(arguments, key: "constraints")
        )
        let createdAt = now()
        do {
            let primary = sources[0]
            let todo = try await store.createAgentTodo(
                ownerUserID: context.ownerUserID,
                agentID: assigneeID,
                requestKey: call.id,
                draft: .init(
                    title: try Self.requiredString(arguments, key: "title"),
                    detail: try Self.optionalString(arguments, key: "detail") ?? "",
                    priority: Int(try Self.optionalInteger(arguments, key: "priority") ?? 50),
                    teamRoomID: teamID,
                    sourceRoomID: primary.roomID,
                    sourceMessageID: primary.messageID,
                    additionalSources: Array(sources.dropFirst()),
                    dependencies: dependencies,
                    executionPlan: .init(
                        requiresExecution: requiresExecution,
                        builtinCapabilities: builtinCapabilities,
                        plugins: pluginSelections,
                        selectionRevision: "local-capability-catalog-v1",
                        selectedAtUnixMs: createdAt
                    ),
                    executionContract: executionContract,
                    creatorAgentID: context.agentID
                ),
                nowUnixMs: createdAt
            )
            if assigneeID != context.agentID {
                _ = try await store.enqueueAgentTodoReady(
                    ownerUserID: context.ownerUserID,
                    agentID: assigneeID,
                    todoID: todo.id,
                    nowUnixMs: createdAt
                )
            }
            return try Self.outcome(try await todoResponse(todo))
        } catch let error as AgentGroupChatError {
            return Self.structuredFailure(
                code: Self.errorCode(error),
                field: Self.errorField(error),
                message: error.localizedDescription,
                retryable: error != .permissionDenied
            )
        }
    }

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
                    message: "补充来源消息无效、已过期，或不属于当前 Agent 的本轮收件箱。",
                    retryable: true,
                    nextTool: Self.readAllUnreadToolName
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

    func reorderTodos(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let todoReferences = try Self.optionalStringArray(arguments, key: "todo_refs")
        guard !todoReferences.isEmpty else {
            throw AgentGroupChatError.invalidField("todo_refs")
        }
        var todoIDs: [String] = []
        var teamRoomID: String?
        for reference in todoReferences {
            guard let authority = await references.todoAuthority(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_todo_ref",
                    field: "todo_refs",
                    message: "Todo 引用无效或已经过期，请重新调用 todo_list。",
                    retryable: true,
                    nextTool: Self.todoListToolName
                )
            }
            if let teamRoomID, teamRoomID != authority.teamRoomID {
                return Self.structuredFailure(
                    code: "cross_team_reorder",
                    field: "todo_refs",
                    message: "一次只能调整同一个团队任务板的顺序。",
                    retryable: true
                )
            }
            teamRoomID = authority.teamRoomID
            todoIDs.append(authority.todoID)
        }
        guard let teamRoomID, try await isProjectManager(teamRoomID: teamRoomID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "todo_refs",
                message: "只有该团队明确指定的项目经理可以调整团队任务顺序。",
                retryable: false
            )
        }
        let todos = try await store.reorderTeamTodos(
            ownerUserID: context.ownerUserID,
            teamRoomID: teamRoomID,
            todoIDs: todoIDs,
            nowUnixMs: now()
        )
        var response: [TodoResponse] = []
        for todo in todos { response.append(try await todoResponse(todo)) }
        return try Self.outcome(response)
    }

}
