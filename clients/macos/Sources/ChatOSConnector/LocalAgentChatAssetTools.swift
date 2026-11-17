import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    func listTeamAssets(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let teamRoomID: String
        if context.lane == .executor {
            guard let todo = try await currentExecutionTodo() else {
                return Self.structuredFailure(
                    code: "todo_execution_context_mismatch",
                    field: "delivery",
                    message: "当前执行线程没有有效的团队资产边界。",
                    retryable: false
                )
            }
            teamRoomID = todo.teamRoomID
        } else if let teamReference = try Self.optionalString(arguments, key: "team_ref") {
            guard let resolved = await references.teamID(reference: teamReference) else {
                return Self.structuredFailure(
                    code: "invalid_team_ref",
                    field: "team_ref",
                    message: "团队引用无效或已经过期，请重新调用 agent_workspace_snapshot。",
                    retryable: true,
                    nextTool: Self.workspaceSnapshotToolName
                )
            }
            teamRoomID = resolved
        } else if try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID
        )?.conversationKind == .projectTeam {
            teamRoomID = context.roomID
        } else {
            return Self.structuredFailure(
                code: "team_ref_required",
                field: "team_ref",
                message: "当前是私聊，请先调用 agent_workspace_snapshot 并选择一个项目团队。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        guard try await isTeamMember(teamRoomID: teamRoomID) else {
            return Self.structuredFailure(
                code: "team_membership_required",
                field: "team_ref",
                message: "当前 Agent 不是该团队成员，不能读取团队共享资产。",
                retryable: false
            )
        }
        var response: [TeamAssetSummaryResponse] = []
        if context.lane == .executor, let todo = try await currentExecutionTodo() {
            for snapshot in try await store.listTodoTeamAssetSnapshots(
                ownerUserID: context.ownerUserID,
                todoID: todo.id
            ) {
                response.append(.init(
                    assetReference: await references.teamAssetReference(
                        assetID: snapshot.assetID,
                        teamRoomID: snapshot.teamRoomID,
                        revision: snapshot.revision
                    ),
                    category: snapshot.category.rawValue,
                    title: snapshot.title,
                    revision: snapshot.revision,
                    updatedAtUnixMs: snapshot.capturedAtUnixMs
                ))
            }
        } else {
            for asset in try await store.listTeamAssets(
                ownerUserID: context.ownerUserID,
                teamRoomID: teamRoomID,
                includeArchived: false
            ) {
                response.append(.init(
                    assetReference: await references.teamAssetReference(
                        assetID: asset.id,
                        teamRoomID: asset.teamRoomID,
                        revision: asset.revision
                    ),
                    category: asset.category.rawValue,
                    title: asset.title,
                    revision: asset.revision,
                    updatedAtUnixMs: asset.updatedAtUnixMs
                ))
            }
        }
        return try Self.outcome(response)
    }

    func getTeamAsset(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let reference = try Self.requiredString(arguments, key: "asset_ref")
        guard let authority = await references.teamAssetAuthority(reference: reference),
              try await isTeamMember(teamRoomID: authority.teamRoomID) else {
            return Self.structuredFailure(
                code: "invalid_team_asset_ref",
                field: "asset_ref",
                message: "团队资产引用无效、已归档或已经过期，请重新调用 team_asset_list。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        if context.lane == .executor, let todo = try await currentExecutionTodo() {
            guard let snapshot = try await store.todoTeamAssetSnapshot(
                ownerUserID: context.ownerUserID,
                todoID: todo.id,
                assetID: authority.assetID,
                revision: authority.revision
            ) else {
                return Self.structuredFailure(
                    code: "invalid_team_asset_ref",
                    field: "asset_ref",
                    message: "该资产不属于当前 Todo 启动时固化的团队上下文。",
                    retryable: true,
                    nextTool: Self.teamAssetListToolName
                )
            }
            return try Self.outcome(TeamAssetDetailResponse(
                assetReference: reference,
                category: snapshot.category.rawValue,
                title: snapshot.title,
                markdown: snapshot.markdown,
                revision: snapshot.revision,
                updatedAtUnixMs: snapshot.capturedAtUnixMs
            ))
        }
        guard let asset = try await store.teamAsset(
            ownerUserID: context.ownerUserID,
            teamRoomID: authority.teamRoomID,
            assetID: authority.assetID
        ), asset.status == .active else {
            return Self.structuredFailure(
                code: "invalid_team_asset_ref",
                field: "asset_ref",
                message: "团队资产已经归档，请重新调用 team_asset_list。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        guard asset.revision == authority.revision else {
            return Self.structuredFailure(
                code: "team_asset_revision_changed",
                field: "asset_ref",
                message: "团队资产已经产生新修订，请重新调用 team_asset_list 后读取最新版本。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        return try Self.outcome(TeamAssetDetailResponse(
            assetReference: reference,
            category: asset.category.rawValue,
            title: asset.title,
            markdown: asset.markdown,
            revision: asset.revision,
            updatedAtUnixMs: asset.updatedAtUnixMs
        ))
    }

    func createTeamAsset(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard context.lane == .manager else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let teamReference = try Self.requiredString(arguments, key: "team_ref")
        guard let teamRoomID = await references.teamID(reference: teamReference) else {
            return Self.structuredFailure(
                code: "invalid_team_ref",
                field: "team_ref",
                message: "团队引用无效或已经过期，请重新调用 agent_workspace_snapshot。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        guard try await isProjectManager(teamRoomID: teamRoomID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "team_ref",
                message: "只有该团队明确指定的项目经理可以维护共享资产。",
                retryable: false
            )
        }
        guard let category = LocalAgentTeamAssetCategory(
            rawValue: try Self.requiredString(arguments, key: "category")
        ) else {
            return Self.structuredFailure(
                code: "invalid_team_asset_category",
                field: "category",
                message: "共享资产分类无效。",
                retryable: true
            )
        }
        let asset = try await store.upsertTeamAsset(
            ownerUserID: context.ownerUserID,
            teamRoomID: teamRoomID,
            assetID: nil,
            editorAgentID: context.agentID,
            category: category,
            title: try Self.requiredString(arguments, key: "title"),
            markdown: try Self.requiredString(arguments, key: "markdown"),
            expectedRevision: nil,
            nowUnixMs: now()
        )
        return try await teamAssetOutcome(asset)
    }

    func updateTeamAsset(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard context.lane == .manager else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let assetReference = try Self.requiredString(arguments, key: "asset_ref")
        guard let authority = await references.teamAssetAuthority(reference: assetReference) else {
            return Self.structuredFailure(
                code: "invalid_team_asset_ref",
                field: "asset_ref",
                message: "团队资产引用无效或已经过期，请重新调用 team_asset_list。空目录首次创建必须改用 team_asset_create。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        guard try await isProjectManager(teamRoomID: authority.teamRoomID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "asset_ref",
                message: "只有该团队明确指定的项目经理可以维护共享资产。",
                retryable: false
            )
        }
        guard let category = LocalAgentTeamAssetCategory(
            rawValue: try Self.requiredString(arguments, key: "category")
        ) else {
            return Self.structuredFailure(
                code: "invalid_team_asset_category",
                field: "category",
                message: "共享资产分类无效。",
                retryable: true
            )
        }
        let asset = try await store.upsertTeamAsset(
            ownerUserID: context.ownerUserID,
            teamRoomID: authority.teamRoomID,
            assetID: authority.assetID,
            editorAgentID: context.agentID,
            category: category,
            title: try Self.requiredString(arguments, key: "title"),
            markdown: try Self.requiredString(arguments, key: "markdown"),
            expectedRevision: authority.revision,
            nowUnixMs: now()
        )
        return try await teamAssetOutcome(asset)
    }

    private func teamAssetOutcome(
        _ asset: LocalAgentTeamAsset
    ) async throws -> AgentToolOutcome {
        let reference = await references.teamAssetReference(
            assetID: asset.id,
            teamRoomID: asset.teamRoomID,
            revision: asset.revision
        )
        return try Self.outcome(TeamAssetDetailResponse(
            assetReference: reference,
            category: asset.category.rawValue,
            title: asset.title,
            markdown: asset.markdown,
            revision: asset.revision,
            updatedAtUnixMs: asset.updatedAtUnixMs
        ))
    }

    func archiveTeamAsset(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard context.lane == .manager else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let reference = try Self.requiredString(arguments, key: "asset_ref")
        guard let authority = await references.teamAssetAuthority(reference: reference) else {
            return Self.structuredFailure(
                code: "invalid_team_asset_ref",
                field: "asset_ref",
                message: "团队资产引用无效或已经过期，请重新调用 team_asset_list。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        guard try await isProjectManager(teamRoomID: authority.teamRoomID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "asset_ref",
                message: "只有该团队明确指定的项目经理可以归档共享资产。",
                retryable: false
            )
        }
        let asset = try await store.archiveTeamAsset(
            ownerUserID: context.ownerUserID,
            teamRoomID: authority.teamRoomID,
            assetID: authority.assetID,
            editorAgentID: context.agentID,
            expectedRevision: authority.revision,
            nowUnixMs: now()
        )
        return try Self.outcome(["status": asset.status.rawValue])
    }

    func listTeamAssetReviews(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard context.lane == .manager else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let teamRoomID: String?
        if let teamReference = try Self.optionalString(arguments, key: "team_ref") {
            guard let resolved = await references.teamID(reference: teamReference) else {
                return Self.structuredFailure(
                    code: "invalid_team_ref",
                    field: "team_ref",
                    message: "团队引用无效或已经过期，请重新调用 agent_workspace_snapshot。",
                    retryable: true,
                    nextTool: Self.workspaceSnapshotToolName
                )
            }
            guard try await isProjectManager(teamRoomID: resolved) else {
                return Self.structuredFailure(
                    code: "project_manager_required",
                    field: "team_ref",
                    message: "只有该团队明确指定的项目经理可以审核共享资产建议。",
                    retryable: false
                )
            }
            teamRoomID = resolved
        } else {
            teamRoomID = nil
        }
        let reviews = try await store.listTeamAssetReviews(
            ownerUserID: context.ownerUserID,
            teamRoomID: teamRoomID,
            reviewerAgentID: context.agentID,
            status: .pending,
            limit: 500
        )
        return try Self.outcome(try await teamAssetReviewResponses(reviews))
    }

    func resolveTeamAssetReview(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard context.lane == .manager else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let reviewReference = try Self.requiredString(arguments, key: "review_ref")
        guard let reviewAuthority = await references.teamAssetReviewAuthority(
            reference: reviewReference
        ), let review = try await store.teamAssetReview(
            ownerUserID: context.ownerUserID,
            reviewID: reviewAuthority.reviewID
        ), review.teamRoomID == reviewAuthority.teamRoomID else {
            return Self.structuredFailure(
                code: "invalid_team_asset_review_ref",
                field: "review_ref",
                message: "审核引用无效或已经过期，请重新调用 team_asset_review_list。",
                retryable: true,
                nextTool: Self.teamAssetReviewListToolName
            )
        }
        guard try await isProjectManager(teamRoomID: review.teamRoomID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "review_ref",
                message: "只有该团队明确指定的项目经理可以处理共享资产建议。",
                retryable: false
            )
        }

        let outcome = try Self.requiredString(arguments, key: "outcome")
        let status: LocalAgentTeamAssetReviewStatus
        let resolvedAssetID: String?
        let resolvedAssetRevision: Int?
        let reason = try Self.optionalString(arguments, key: "reason") ?? ""
        switch outcome {
        case LocalAgentTeamAssetReviewStatus.applied.rawValue:
            let assetReference = try Self.requiredString(arguments, key: "asset_ref")
            guard let asset = await references.teamAssetAuthority(reference: assetReference),
                  asset.teamRoomID == review.teamRoomID else {
                return Self.structuredFailure(
                    code: "invalid_team_asset_ref",
                    field: "asset_ref",
                    message: "必须提供同一团队刚由 team_asset_create/update 返回的当前资产引用。",
                    retryable: true,
                    nextTool: Self.teamAssetListToolName
                )
            }
            status = .applied
            resolvedAssetID = asset.assetID
            resolvedAssetRevision = asset.revision
        case LocalAgentTeamAssetReviewStatus.rejected.rawValue:
            guard !reason.isEmpty else {
                return Self.structuredFailure(
                    code: "team_asset_review_reason_required",
                    field: "reason",
                    message: "拒绝共享资产建议必须给出可追溯的事实理由。",
                    retryable: true
                )
            }
            guard try Self.optionalString(arguments, key: "asset_ref") == nil else {
                return Self.structuredFailure(
                    code: "team_asset_review_asset_not_allowed",
                    field: "asset_ref",
                    message: "拒绝建议时不要提供 asset_ref。",
                    retryable: true
                )
            }
            status = .rejected
            resolvedAssetID = nil
            resolvedAssetRevision = nil
        default:
            return Self.structuredFailure(
                code: "invalid_team_asset_review_outcome",
                field: "outcome",
                message: "outcome 只能是 applied 或 rejected。",
                retryable: true
            )
        }
        let updated = try await store.resolveTeamAssetReview(
            ownerUserID: context.ownerUserID,
            reviewID: review.id,
            reviewerAgentID: context.agentID,
            status: status,
            resolvedAssetID: resolvedAssetID,
            resolvedAssetRevision: resolvedAssetRevision,
            reason: reason,
            nowUnixMs: now()
        )
        return try Self.outcome(try await teamAssetReviewResponse(updated))
    }

    private func teamAssetReviewResponses(
        _ reviews: [LocalAgentTeamAssetReview]
    ) async throws -> [TeamAssetReviewResponse] {
        let todos = try await store.todos(
            ownerUserID: context.ownerUserID,
            todoIDs: reviews.map(\.todoID)
        )
        var responses: [TeamAssetReviewResponse] = []
        for review in reviews {
            responses.append(try await teamAssetReviewResponse(review, todo: todos[review.todoID]))
        }
        return responses
    }

    private func teamAssetReviewResponse(
        _ review: LocalAgentTeamAssetReview,
        todo: LocalAgentTodo? = nil
    ) async throws -> TeamAssetReviewResponse {
        let sourceTodo = if let todo {
            todo
        } else {
            try await store.todos(
                ownerUserID: context.ownerUserID,
                todoIDs: [review.todoID]
            )[review.todoID]
        }
        let resolvedReference: String?
        if let assetID = review.resolvedAssetID, let revision = review.resolvedAssetRevision {
            resolvedReference = await references.teamAssetReference(
                assetID: assetID,
                teamRoomID: review.teamRoomID,
                revision: revision
            )
        } else {
            resolvedReference = nil
        }
        return TeamAssetReviewResponse(
            reviewReference: await references.teamAssetReviewReference(
                reviewID: review.id,
                teamRoomID: review.teamRoomID
            ),
            teamReference: await references.teamReference(teamID: review.teamRoomID),
            todoReference: await references.todoReference(
                todoID: review.todoID,
                agentID: review.sourceAgentID,
                teamRoomID: review.teamRoomID
            ),
            sourceAgentReference: await references.agentReference(agentID: review.sourceAgentID),
            sourceTodoTitle: sourceTodo?.title ?? "",
            category: review.suggestion.category.rawValue,
            title: review.suggestion.title,
            markdown: review.suggestion.markdown,
            rationale: review.suggestion.rationale,
            status: review.status.rawValue,
            resolvedAssetReference: resolvedReference,
            reviewReason: review.reviewReason,
            createdAtUnixMs: review.createdAtUnixMs
        )
    }

}
