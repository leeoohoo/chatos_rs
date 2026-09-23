import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
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
                    message: "来源消息引用不是当前 Run 的工具签发值、已损坏，或不属于当前 Agent。若来源是本次唤醒消息，请重新调用 chat_get_trigger；若来源是新收件箱消息，请调用 chat_read_all_unread。",
                    retryable: true,
                    nextTool: Self.getTriggerToolName
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
        builtinCapabilities = LocalAgentTodoExecutionPlan.completingDependencies(
            in: builtinCapabilities
        )
        let requiresExecution = try Self.optionalBoolean(
            arguments,
            key: "requires_execution"
        ) ?? true
        if !requiresExecution,
           builtinCapabilities.contains(where: {
               $0 != .projectRead && $0 != .requirementSurveyRead
           }) {
            return Self.structuredFailure(
                code: "execution_required",
                field: "requires_execution",
                message: "写入类能力或终端能力需要执行环境，请将 requires_execution 设为 true。",
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

}
