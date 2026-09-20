import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
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
