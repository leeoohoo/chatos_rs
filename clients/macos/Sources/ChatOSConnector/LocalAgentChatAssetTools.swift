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

    func upsertTeamAsset(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard context.lane == .manager else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let assetReference = try Self.optionalString(arguments, key: "asset_ref")
        let authority: LocalAgentRunReferenceVault.TeamAssetAuthority? = if let assetReference {
            await references.teamAssetAuthority(reference: assetReference)
        } else {
            nil
        }
        if assetReference != nil, authority == nil {
            return Self.structuredFailure(
                code: "invalid_team_asset_ref",
                field: "asset_ref",
                message: "团队资产引用无效或已经过期，请重新调用 team_asset_list。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        let teamRoomID: String
        if let authority {
            teamRoomID = authority.teamRoomID
        } else {
            let teamReference = try Self.requiredString(arguments, key: "team_ref")
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
        let expectedRevision = try Self.optionalInteger(arguments, key: "expected_revision").map(Int.init)
        if let authority, expectedRevision != authority.revision {
            return Self.structuredFailure(
                code: "team_asset_revision_required",
                field: "expected_revision",
                message: "更新共享资产必须使用 team_asset_list 返回的当前 revision。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        let asset = try await store.upsertTeamAsset(
            ownerUserID: context.ownerUserID,
            teamRoomID: teamRoomID,
            assetID: authority?.assetID,
            editorAgentID: context.agentID,
            category: category,
            title: try Self.requiredString(arguments, key: "title"),
            markdown: try Self.requiredString(arguments, key: "markdown"),
            expectedRevision: expectedRevision,
            nowUnixMs: now()
        )
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

}
