@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
    func testProjectDashboardRequiresManagerAndUsesOptimisticRevisioning() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        XCTAssertEqual(
            try sqliteInt(
                url,
                sql: "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'local_agent_project_dashboards'"
            ),
            1
        )

        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护项目总览。",
                modelConfigID: "model-1",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(store, name: "执行者")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "dashboard-project",
            draft: .init(name: "总览团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: worker.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "dashboard-todo",
            draft: .init(
                title: "完成第一阶段",
                teamRoomID: room.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 100
        )
        let firstUpdate = LocalAgentProjectDashboardUpdate(
            phase: "实现阶段",
            health: .onTrack,
            summary: "核心功能正在实现。",
            nextSteps: ["完成联调"],
            milestones: [
                .init(
                    id: "milestone-1",
                    title: "第一阶段",
                    status: .inProgress,
                    progressPercent: 60,
                    linkedTodoIDs: [todo.id]
                ),
            ],
            issues: [
                .init(
                    id: "issue-1",
                    title: "等待确认",
                    requestedAction: "确认验收范围",
                    severity: .warning,
                    owner: .human,
                    relatedTodoID: todo.id
                ),
            ]
        )

        let created = try await store.upsertProjectDashboard(
            ownerUserID: "alice",
            teamRoomID: room.id,
            editorAgentID: manager.id,
            expectedRevision: nil,
            update: firstUpdate,
            nowUnixMs: 101
        )
        XCTAssertEqual(created.revision, 1)
        XCTAssertEqual(created.milestones.first?.linkedTodoIDs, [todo.id])
        let loaded = try await store.projectDashboard(
            ownerUserID: "alice",
            teamRoomID: room.id
        )
        XCTAssertEqual(loaded, created)

        let secondUpdate = LocalAgentProjectDashboardUpdate(
            phase: "联调阶段",
            health: .atRisk,
            summary: "进入联调，存在一项待确认事项。",
            nextSteps: ["完成联调", "确认验收范围"],
            milestones: firstUpdate.milestones,
            issues: firstUpdate.issues
        )
        let updated = try await store.upsertProjectDashboard(
            ownerUserID: "alice",
            teamRoomID: room.id,
            editorAgentID: manager.id,
            expectedRevision: 1,
            update: secondUpdate,
            nowUnixMs: 102
        )
        XCTAssertEqual(updated.revision, 2)
        XCTAssertEqual(updated.phase, "联调阶段")

        do {
            _ = try await store.upsertProjectDashboard(
                ownerUserID: "alice",
                teamRoomID: room.id,
                editorAgentID: manager.id,
                expectedRevision: 1,
                update: secondUpdate,
                nowUnixMs: 103
            )
            XCTFail("A stale dashboard revision was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }

        do {
            _ = try await store.upsertProjectDashboard(
                ownerUserID: "alice",
                teamRoomID: room.id,
                editorAgentID: worker.id,
                expectedRevision: 2,
                update: secondUpdate,
                nowUnixMs: 104
            )
            XCTFail("A non-manager updated the dashboard")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }

        let otherRoom = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "other-dashboard-project",
            draft: .init(name: "其他团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: otherRoom.id,
            agentID: worker.id,
            draft: .init(role: "执行者")
        )
        let otherTodo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "other-dashboard-todo",
            draft: .init(
                title: "其他团队任务",
                teamRoomID: otherRoom.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 105
        )
        let invalidUpdate = LocalAgentProjectDashboardUpdate(
            phase: "联调阶段",
            health: .atRisk,
            summary: "不应允许跨团队任务引用。",
            milestones: [
                .init(
                    id: "milestone-2",
                    title: "错误引用",
                    status: .pending,
                    progressPercent: 0,
                    linkedTodoIDs: [otherTodo.id]
                ),
            ]
        )
        do {
            _ = try await store.upsertProjectDashboard(
                ownerUserID: "alice",
                teamRoomID: room.id,
                editorAgentID: manager.id,
                expectedRevision: 2,
                update: invalidUpdate,
                nowUnixMs: 106
            )
            XCTFail("A dashboard linked a Todo from another team")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .invalidField("dashboardTodoRefs"))
        }
        let unchanged = try await store.projectDashboard(
            ownerUserID: "alice",
            teamRoomID: room.id
        )
        XCTAssertEqual(unchanged?.revision, 2)

        let verboseUpdate = LocalAgentProjectDashboardUpdate(
            phase: "联调阶段",
            health: .onTrack,
            summary: String(repeating: "过长的内部技术记录", count: 100)
        )
        do {
            _ = try await store.upsertProjectDashboard(
                ownerUserID: "alice",
                teamRoomID: room.id,
                editorAgentID: manager.id,
                expectedRevision: 2,
                update: verboseUpdate,
                nowUnixMs: 107
            )
            XCTFail("An oversized manager brief was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .invalidField("dashboardSummary"))
        }

        let noisyStepsUpdate = LocalAgentProjectDashboardUpdate(
            phase: "联调阶段",
            health: .onTrack,
            summary: "面向用户的项目结论。",
            nextSteps: Array(repeating: "一个明确动作", count: 6)
        )
        do {
            _ = try await store.upsertProjectDashboard(
                ownerUserID: "alice",
                teamRoomID: room.id,
                editorAgentID: manager.id,
                expectedRevision: 2,
                update: noisyStepsUpdate,
                nowUnixMs: 108
            )
            XCTFail("Too many dashboard next steps were accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .invalidField("dashboardNextSteps"))
        }
    }

    func testTodoCompletionProgressPersistsAssetSuggestionsAndStatusCallsThemOut() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护任务与资产。",
                modelConfigID: "model-1",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(store, name: "执行者")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "suggestion-project",
            draft: .init(name: "建议团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: worker.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "suggestion-todo",
            draft: .init(
                title: "完成架构验证",
                teamRoomID: room.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 200
        )
        let suggestion = LocalAgentTeamAssetUpdateSuggestion(
            category: .architecture,
            title: "架构决策",
            markdown: "采用事件驱动更新。\n",
            rationale: "执行验证已确认该方案通过验收。"
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            update: .init(status: .inProgress),
            nowUnixMs: 200
        )
        _ = try await store.appendAgentTodoProgress(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            kind: .completed,
            runID: "run-suggestion",
            stage: "completed",
            detail: "验证完成",
            assetUpdateSuggestions: [suggestion],
            nowUnixMs: 201
        )
        _ = try await store.appendAgentTodoProgress(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            kind: .completed,
            runID: "run-suggestion-replay",
            stage: "completed",
            detail: "重放同一完成结果",
            assetUpdateSuggestions: [suggestion],
            nowUnixMs: 202
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            update: .init(status: .completed, result: "验证完成"),
            nowUnixMs: 203
        )
        let progress = try await store.listAgentTodoProgress(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            limit: 10
        )
        XCTAssertEqual(progress.last?.assetUpdateSuggestions, [suggestion])
        let reviews = try await store.listTeamAssetReviews(
            ownerUserID: "alice",
            teamRoomID: room.id,
            reviewerAgentID: manager.id,
            status: .pending,
            limit: 10
        )
        XCTAssertEqual(reviews.count, 1)
        XCTAssertEqual(
            reviews.first?.todoEventID,
            progress.first(where: { !$0.assetUpdateSuggestions.isEmpty })?.id
        )
        XCTAssertEqual(reviews.first?.suggestion, suggestion)
        let deliveries = try await store.enqueueAgentTodoStatus(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            excludingAgentID: worker.id,
            nowUnixMs: 204
        )
        let managerStatus = try XCTUnwrap(deliveries.first)
        let message = try await store.message(
            ownerUserID: "alice",
            roomID: managerStatus.roomID,
            messageID: managerStatus.messageID
        )
        XCTAssertTrue(try XCTUnwrap(message).content.contains("共享资产更新建议：1 条"))
    }

    func testTeamAssetReviewRequiresManagerCurrentRevisionAndExplicitRejection() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "审核团队资产。",
                modelConfigID: "model",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(store, name: "执行者")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "asset-review-project",
            draft: .init(name: "资产审核团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: worker.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "asset-review-todo",
            draft: .init(title: "验证架构", teamRoomID: room.id, creatorAgentID: manager.id),
            nowUnixMs: 100
        )
        _ = try await store.appendAgentTodoProgress(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            kind: .completed,
            runID: "asset-review-run",
            stage: "completed",
            detail: "完成并提交两条建议",
            assetUpdateSuggestions: [
                .init(
                    category: .architecture,
                    title: "事件驱动架构",
                    markdown: "采用事件驱动。\n",
                    rationale: "已通过验证。"
                ),
                .init(
                    category: .currentProgress,
                    title: "临时进度",
                    markdown: "一次性状态。\n",
                    rationale: "执行过程生成。"
                ),
            ],
            nowUnixMs: 101
        )
        let reviews = try await store.listTeamAssetReviews(
            ownerUserID: "alice",
            teamRoomID: room.id,
            reviewerAgentID: manager.id,
            status: .pending,
            limit: 10
        )
        XCTAssertEqual(reviews.count, 2)

        let firstAsset = try await store.upsertTeamAsset(
            ownerUserID: "alice",
            teamRoomID: room.id,
            assetID: nil,
            editorAgentID: manager.id,
            category: .architecture,
            title: "事件驱动架构",
            markdown: "采用事件驱动。\n",
            expectedRevision: nil,
            nowUnixMs: 102
        )
        XCTAssertEqual(firstAsset.markdown, "采用事件驱动。\n")
        let currentAsset = try await store.upsertTeamAsset(
            ownerUserID: "alice",
            teamRoomID: room.id,
            assetID: firstAsset.id,
            editorAgentID: manager.id,
            category: .architecture,
            title: "事件驱动架构",
            markdown: "采用事件驱动，并记录边界。\n",
            expectedRevision: firstAsset.revision,
            nowUnixMs: 103
        )
        XCTAssertEqual(currentAsset.markdown, "采用事件驱动，并记录边界。\n")

        do {
            _ = try await store.resolveTeamAssetReview(
                ownerUserID: "alice",
                reviewID: reviews[0].id,
                reviewerAgentID: worker.id,
                status: .applied,
                resolvedAssetID: currentAsset.id,
                resolvedAssetRevision: currentAsset.revision,
                reason: "",
                nowUnixMs: 104
            )
            XCTFail("A non-manager resolved an asset review")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }
        do {
            _ = try await store.resolveTeamAssetReview(
                ownerUserID: "alice",
                reviewID: reviews[0].id,
                reviewerAgentID: manager.id,
                status: .applied,
                resolvedAssetID: firstAsset.id,
                resolvedAssetRevision: firstAsset.revision,
                reason: "",
                nowUnixMs: 105
            )
            XCTFail("A stale asset revision resolved a review")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }
        let applied = try await store.resolveTeamAssetReview(
            ownerUserID: "alice",
            reviewID: reviews[0].id,
            reviewerAgentID: manager.id,
            status: .applied,
            resolvedAssetID: currentAsset.id,
            resolvedAssetRevision: currentAsset.revision,
            reason: "",
            nowUnixMs: 106
        )
        XCTAssertEqual(applied.status, .applied)
        XCTAssertEqual(applied.resolvedAssetRevision, currentAsset.revision)
        let replayedApplied = try await store.resolveTeamAssetReview(
            ownerUserID: "alice",
            reviewID: reviews[0].id,
            reviewerAgentID: manager.id,
            status: .applied,
            resolvedAssetID: currentAsset.id,
            resolvedAssetRevision: currentAsset.revision,
            reason: "",
            nowUnixMs: 107
        )
        XCTAssertEqual(replayedApplied, applied)

        do {
            _ = try await store.resolveTeamAssetReview(
                ownerUserID: "alice",
                reviewID: reviews[1].id,
                reviewerAgentID: manager.id,
                status: .rejected,
                resolvedAssetID: nil,
                resolvedAssetRevision: nil,
                reason: "",
                nowUnixMs: 108
            )
            XCTFail("A rejection without a reason was accepted")
        } catch {
            XCTAssertEqual(
                error as? AgentGroupChatError,
                .invalidField("teamAssetReviewReason")
            )
        }
        let rejected = try await store.resolveTeamAssetReview(
            ownerUserID: "alice",
            reviewID: reviews[1].id,
            reviewerAgentID: manager.id,
            status: .rejected,
            resolvedAssetID: nil,
            resolvedAssetRevision: nil,
            reason: "这是一次性状态，Todo 进度已完整保留。",
            nowUnixMs: 109
        )
        XCTAssertEqual(rejected.status, .rejected)
        let remaining = try await store.listTeamAssetReviews(
            ownerUserID: "alice",
            teamRoomID: room.id,
            reviewerAgentID: manager.id,
            status: .pending,
            limit: 10
        )
        XCTAssertTrue(remaining.isEmpty)
    }
}
