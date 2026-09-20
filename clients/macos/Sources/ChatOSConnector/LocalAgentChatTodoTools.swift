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

}
