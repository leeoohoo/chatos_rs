import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    private struct DashboardTodoResponse: Encodable {
        let todoRef: String
        let title: String
        let assignee: String
        let status: String
        let blockedReason: String
        let updatedAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case todoRef = "todo_ref"
            case title, assignee, status
            case blockedReason = "blocked_reason"
            case updatedAtUnixMs = "updated_at_unix_ms"
        }
    }

    private struct DashboardMilestoneResponse: Encodable {
        let id: String
        let title: String
        let detail: String
        let status: String
        let progressPercent: Int
        let acceptanceCriteria: [String]
        let todoRefs: [String]
        let targetAtUnixMs: Int64?

        enum CodingKeys: String, CodingKey {
            case id, title, detail, status
            case progressPercent = "progress_percent"
            case acceptanceCriteria = "acceptance_criteria"
            case todoRefs = "todo_refs"
            case targetAtUnixMs = "target_at_unix_ms"
        }
    }

    private struct DashboardIssueResponse: Encodable {
        let id: String
        let title: String
        let detail: String
        let requestedAction: String
        let severity: String
        let owner: String
        let todoRef: String?

        enum CodingKeys: String, CodingKey {
            case id, title, detail, severity, owner
            case requestedAction = "requested_action"
            case todoRef = "todo_ref"
        }
    }

    private struct DashboardRecordResponse: Encodable {
        let revision: Int
        let phase: String
        let health: String
        let summary: String
        let nextSteps: [String]
        let milestones: [DashboardMilestoneResponse]
        let issues: [DashboardIssueResponse]
        let updatedAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case revision, phase, health, summary, milestones, issues
            case nextSteps = "next_steps"
            case updatedAtUnixMs = "updated_at_unix_ms"
        }
    }

    private struct DashboardFactsResponse: Encodable {
        let totalTodos: Int
        let pendingTodos: Int
        let inProgressTodos: Int
        let blockedTodos: Int
        let completedTodos: Int
        let pendingSurveys: Int
        let pendingApprovals: Int

        enum CodingKeys: String, CodingKey {
            case totalTodos = "total_todos"
            case pendingTodos = "pending_todos"
            case inProgressTodos = "in_progress_todos"
            case blockedTodos = "blocked_todos"
            case completedTodos = "completed_todos"
            case pendingSurveys = "pending_surveys"
            case pendingApprovals = "pending_approvals"
        }
    }

    private struct ProjectDashboardResponse: Encodable {
        let teamRef: String
        let teamName: String
        let teamGoal: String
        let dashboard: DashboardRecordResponse?
        let facts: DashboardFactsResponse
        let todos: [DashboardTodoResponse]

        enum CodingKeys: String, CodingKey {
            case teamRef = "team_ref"
            case teamName = "team_name"
            case teamGoal = "team_goal"
            case dashboard, facts, todos
        }
    }

    func getProjectDashboard(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        guard let team = try await resolveDashboardTeam(arguments: arguments) else {
            return Self.structuredFailure(
                code: "team_ref_required",
                field: "team_ref",
                message: "当前会话不是项目团队，请先调用 agent_workspace_snapshot 获取 team_ref。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        let members = try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: team.id
        )
        guard members.contains(where: { $0.agentID == context.agentID && $0.status == .active }) else {
            throw AgentGroupChatError.notMember
        }
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let names = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0.draft.name) })
        let todos = try await store.listTeamTodos(
            ownerUserID: context.ownerUserID,
            teamRoomID: team.id,
            includeTerminal: true
        )
        var todoResponses: [DashboardTodoResponse] = []
        var todoRefsByID: [String: String] = [:]
        for todo in todos {
            let reference = await references.todoReference(
                todoID: todo.id,
                agentID: todo.agentID,
                teamRoomID: todo.teamRoomID
            )
            todoRefsByID[todo.id] = reference
            todoResponses.append(.init(
                todoRef: reference,
                title: todo.title,
                assignee: names[todo.agentID] ?? "Agent",
                status: todo.status.rawValue,
                blockedReason: todo.blockedReason,
                updatedAtUnixMs: todo.updatedAtUnixMs
            ))
        }

        let surveys = try await store.listRequirementSurveys(
            ownerUserID: context.ownerUserID,
            projectID: team.projectID,
            status: .pending
        )
        let approvals = try await pendingApprovalCount(teamRoomID: team.id)
        let dashboard = try await store.projectDashboard(
            ownerUserID: context.ownerUserID,
            teamRoomID: team.id
        )
        let currentDashboardResponse: DashboardRecordResponse? = dashboard.map { dashboard in
            self.dashboardResponse(dashboard, todoRefsByID: todoRefsByID)
        }
        return try Self.outcome(ProjectDashboardResponse(
            teamRef: await references.teamReference(teamID: team.id),
            teamName: team.draft.name,
            teamGoal: team.draft.goal,
            dashboard: currentDashboardResponse,
            facts: .init(
                totalTodos: todos.count,
                pendingTodos: todos.filter { $0.status == .pending }.count,
                inProgressTodos: todos.filter { $0.status == .inProgress }.count,
                blockedTodos: todos.filter { $0.status == .blocked }.count,
                completedTodos: todos.filter { $0.status == .completed }.count,
                pendingSurveys: surveys.count,
                pendingApprovals: approvals
            ),
            todos: todoResponses
        ))
    }

    func updateProjectDashboard(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let teamReference = try Self.requiredString(arguments, key: "team_ref")
        guard let teamID = await references.teamID(reference: teamReference) else {
            return Self.structuredFailure(
                code: "invalid_team_ref",
                field: "team_ref",
                message: "团队引用无效或已经过期，请重新调用 project_dashboard_get。",
                retryable: true,
                nextTool: Self.projectDashboardGetToolName
            )
        }
        let expectedRevision = try Self.optionalInteger(arguments, key: "expected_revision").map(Int.init)
        let healthRaw = try Self.requiredString(arguments, key: "health")
        guard let health = LocalAgentProjectHealth(rawValue: healthRaw) else {
            throw AgentGroupChatError.invalidField("health")
        }
        let milestones = try await parseDashboardMilestones(
            Self.optionalObjectArray(arguments, key: "milestones"),
            teamID: teamID
        )
        let issues = try await parseDashboardIssues(
            Self.optionalObjectArray(arguments, key: "issues"),
            teamID: teamID
        )
        let update = LocalAgentProjectDashboardUpdate(
            phase: try Self.requiredString(arguments, key: "phase"),
            health: health,
            summary: try Self.requiredString(arguments, key: "summary"),
            nextSteps: try Self.optionalStringArray(arguments, key: "next_steps"),
            milestones: milestones,
            issues: issues
        )
        do {
            let saved = try await store.upsertProjectDashboard(
                ownerUserID: context.ownerUserID,
                teamRoomID: teamID,
                editorAgentID: context.agentID,
                expectedRevision: expectedRevision,
                update: update,
                nowUnixMs: now()
            )
            await roomChangeHandler(teamID)
            return try await dashboardOutcome(saved)
        } catch AgentGroupChatError.conflict {
            return Self.structuredFailure(
                code: "dashboard_revision_changed",
                field: "expected_revision",
                message: "项目总览已经产生新版本，请重新调用 project_dashboard_get 后合并更新。",
                retryable: true,
                nextTool: Self.projectDashboardGetToolName
            )
        }
    }

    private func resolveDashboardTeam(
        arguments: [String: Any]
    ) async throws -> ProjectAgentRoom? {
        if let reference = try Self.optionalString(arguments, key: "team_ref") {
            guard let teamID = await references.teamID(reference: reference) else { return nil }
            return try await store.room(ownerUserID: context.ownerUserID, roomID: teamID)
        }
        guard let room = try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID
        ), room.conversationKind == .projectTeam else { return nil }
        return room
    }

    private func pendingApprovalCount(teamRoomID: String) async throws -> Int {
        let agents = try await store.listAgentProposals(
            ownerUserID: context.ownerUserID,
            roomID: teamRoomID,
            status: .pending
        ).count
        let removals = try await store.listAgentRemovalProposals(
            ownerUserID: context.ownerUserID,
            roomID: teamRoomID,
            status: .pending
        ).count
        let teams = try await store.listTeamProposals(
            ownerUserID: context.ownerUserID,
            sourceRoomID: teamRoomID,
            status: .pending
        ).count
        let memberships = try await store.listMembershipProposals(
            ownerUserID: context.ownerUserID,
            sourceRoomID: teamRoomID,
            status: .pending
        ).count
        return agents + removals + teams + memberships
    }

    private func dashboardResponse(
        _ dashboard: LocalAgentProjectDashboard,
        todoRefsByID: [String: String]
    ) -> DashboardRecordResponse {
        .init(
            revision: dashboard.revision,
            phase: dashboard.phase,
            health: dashboard.health.rawValue,
            summary: dashboard.summary,
            nextSteps: dashboard.nextSteps,
            milestones: dashboard.milestones.map { milestone in
                .init(
                    id: milestone.id,
                    title: milestone.title,
                    detail: milestone.detail,
                    status: milestone.status.rawValue,
                    progressPercent: milestone.progressPercent,
                    acceptanceCriteria: milestone.acceptanceCriteria,
                    todoRefs: milestone.linkedTodoIDs.compactMap { todoRefsByID[$0] },
                    targetAtUnixMs: milestone.targetAtUnixMs
                )
            },
            issues: dashboard.issues.map { issue in
                .init(
                    id: issue.id,
                    title: issue.title,
                    detail: issue.detail,
                    requestedAction: issue.requestedAction,
                    severity: issue.severity.rawValue,
                    owner: issue.owner.rawValue,
                    todoRef: issue.relatedTodoID.flatMap { todoRefsByID[$0] }
                )
            },
            updatedAtUnixMs: dashboard.updatedAtUnixMs
        )
    }

    private func dashboardOutcome(
        _ dashboard: LocalAgentProjectDashboard
    ) async throws -> AgentToolOutcome {
        let todos = try await store.listTeamTodos(
            ownerUserID: context.ownerUserID,
            teamRoomID: dashboard.teamRoomID,
            includeTerminal: true
        )
        var referencesByID: [String: String] = [:]
        for todo in todos {
            referencesByID[todo.id] = await references.todoReference(
                todoID: todo.id,
                agentID: todo.agentID,
                teamRoomID: todo.teamRoomID
            )
        }
        return try Self.outcome(dashboardResponse(
            dashboard,
            todoRefsByID: referencesByID
        ))
    }

    private func parseDashboardMilestones(
        _ objects: [[String: Any]],
        teamID: String
    ) async throws -> [LocalAgentProjectMilestone] {
        try await objects.asyncMap { object in
            guard let status = LocalAgentProjectMilestoneStatus(
                rawValue: try Self.requiredString(object, key: "status")
            ), let progress = try Self.optionalInteger(object, key: "progress_percent") else {
                throw AgentGroupChatError.invalidField("milestones")
            }
            let todoIDs = try await Self.optionalStringArray(object, key: "todo_refs").asyncMap {
                reference in
                guard let authority = await references.todoAuthority(reference: reference),
                      authority.teamRoomID == teamID else {
                    throw AgentGroupChatError.invalidField("milestones.todo_refs")
                }
                return authority.todoID
            }
            return LocalAgentProjectMilestone(
                id: try Self.requiredString(object, key: "id"),
                title: try Self.requiredString(object, key: "title"),
                detail: try Self.optionalString(object, key: "detail") ?? "",
                status: status,
                progressPercent: Int(progress),
                acceptanceCriteria: try Self.optionalStringArray(object, key: "acceptance_criteria"),
                linkedTodoIDs: todoIDs,
                targetAtUnixMs: try Self.optionalInteger(object, key: "target_at_unix_ms")
            )
        }
    }

    private func parseDashboardIssues(
        _ objects: [[String: Any]],
        teamID: String
    ) async throws -> [LocalAgentProjectIssue] {
        try await objects.asyncMap { object in
            guard let severity = LocalAgentProjectIssueSeverity(
                rawValue: try Self.requiredString(object, key: "severity")
            ), let owner = LocalAgentProjectIssueOwner(
                rawValue: try Self.requiredString(object, key: "owner")
            ) else {
                throw AgentGroupChatError.invalidField("issues")
            }
            let relatedTodoID: String?
            if let reference = try Self.optionalString(object, key: "todo_ref") {
                guard let authority = await references.todoAuthority(reference: reference),
                      authority.teamRoomID == teamID else {
                    throw AgentGroupChatError.invalidField("issues.todo_ref")
                }
                relatedTodoID = authority.todoID
            } else {
                relatedTodoID = nil
            }
            return LocalAgentProjectIssue(
                id: try Self.requiredString(object, key: "id"),
                title: try Self.requiredString(object, key: "title"),
                detail: try Self.optionalString(object, key: "detail") ?? "",
                requestedAction: try Self.requiredString(object, key: "requested_action"),
                severity: severity,
                owner: owner,
                relatedTodoID: relatedTodoID
            )
        }
    }
}

private extension Array {
    func asyncMap<T>(_ transform: (Element) async throws -> T) async rethrows -> [T] {
        var result: [T] = []
        result.reserveCapacity(count)
        for element in self {
            result.append(try await transform(element))
        }
        return result
    }
}
