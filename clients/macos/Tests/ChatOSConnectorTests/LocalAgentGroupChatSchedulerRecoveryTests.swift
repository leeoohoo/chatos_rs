import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

extension LocalAgentGroupChatSchedulerTests {
    func testDirectResumeKeepsUnknownWriteInNeedsReviewUntilHumanExplicitlyRetriesIt() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-resume-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let nativeService = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await nativeService.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(name: "实现者", rolePrompt: "实现任务", modelConfigID: "local-model")
        )
        let room = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "继续"),
            limits: .init()
        )
        let pending = try XCTUnwrap(post.deliveries.first)
        let claimed = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: post.message.createdAtUnixMs + 1
        )
        let delivery = try XCTUnwrap(claimed)
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: room.projectID,
            roomID: room.id,
            agentID: agent.id,
            deliveryID: delivery.id,
            triggerMessageID: post.message.id,
            rootMessageID: post.message.rootMessageID,
            runID: runID.uuidString.lowercased(),
            hopCount: delivery.hopCount
        )
        var checkpoint = AgentRunCheckpoint(
            scope: LocalAgentGroupChatRun.runtimeScope(for: context),
            messages: [.init(role: .system, content: "system")]
        )
        checkpoint.id = runID
        let call = AgentToolCall(
            id: "unknown-write",
            name: LocalAgentChatToolProvider.completeManagerCycleToolName,
            arguments: "{}"
        )
        checkpoint.pendingCalls = [call]
        checkpoint.inFlightCallID = call.id
        checkpoint.status = .running
        checkpoint.elapsedSeconds = 7_200
        let memoryScope = try AgentMemoryScope(
            tenantID: "alice",
            agentID: agent.id,
            projectID: room.projectID,
            runID: runID,
            runtimeScope: checkpoint.scope
        )
        checkpoint.memory = .init(scope: memoryScope, pinnedMessageCount: 1)
        checkpoint.memory?.threadCreated = true
        let run = try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: agent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: post.message.createdAtUnixMs + 1,
            updatedAtUnixMs: post.message.createdAtUnixMs + 1
        )
        try await store.saveRun(run)

        let settingsSuite = "local-agent-resume-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: nativeService,
            services: SchedulerTestServices(),
            settings: .init(suiteName: settingsSuite)
        )
        let result = try await scheduler.resumeDelivery(
            ownerUserID: "alice",
            projectID: room.projectID,
            deliveryID: pending.id
        )
        XCTAssertEqual(result.outcome, .suspended)
        let loadedReviewed = try await store.run(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        let reviewed = try XCTUnwrap(loadedReviewed)
        XCTAssertEqual(reviewed.id, runID)
        XCTAssertEqual(reviewed.checkpoint.status, .needsReview)
        XCTAssertEqual(reviewed.checkpoint.messages.first?.content, "system")
        XCTAssertTrue(reviewed.checkpoint.instructionBundleItems.isEmpty)
        let quarantinedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(quarantinedDelivery?.status, .failed)
        let listedRuns = try await store.listUnfinishedRuns(
            ownerUserID: "alice",
            projectID: room.projectID,
            limit: 10
        )
        XCTAssertEqual(listedRuns.map(\.id), [runID])

        let retried = try await scheduler.retryInterruptedDelivery(
            ownerUserID: "alice",
            projectID: room.projectID,
            deliveryID: delivery.id
        )
        XCTAssertEqual(retried.outcome, .completed)
        let completedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        let completedRun = try await store.run(ownerUserID: "alice", deliveryID: delivery.id)
        let unfinishedAfterRetry = try await store.listUnfinishedRuns(
            ownerUserID: "alice",
            projectID: room.projectID,
            limit: 10
        )
        XCTAssertEqual(completedDelivery?.status, .completed)
        XCTAssertEqual(completedRun?.checkpoint.status, .completed)
        XCTAssertLessThan(completedRun?.checkpoint.elapsedSeconds ?? 7_200, 7_200)
        XCTAssertEqual(completedRun?.checkpoint.messages.first?.content, "system")
        XCTAssertTrue(completedRun?.checkpoint.instructionBundleItems.isEmpty == true)
        XCTAssertTrue(completedRun?.events.contains(where: { $0.kind == "retry_authorized" }) == true)
        XCTAssertTrue(unfinishedAfterRetry.isEmpty)
    }

    func testAbandoningExecutorBlocksTodoAndWakesManager() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-abandon-executor-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let nativeService = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await nativeService.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "管理并执行任务。",
                modelConfigID: "local-model",
                professionKey: "project_manager"
            )
        )
        let team = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "abandon-executor-project",
            draft: .init(name: "执行恢复团队"),
            projectManagerAgentID: agent.id
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "abandon-running-todo",
            draft: .init(
                title: "执行到一半的任务",
                teamRoomID: team.id,
                creatorAgentID: agent.id
            ),
            nowUnixMs: 100
        )
        let pendingValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        let pending = try XCTUnwrap(pendingValue)
        let claimedValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 102
        )
        let delivery = try XCTUnwrap(claimedValue)
        XCTAssertEqual(delivery.id, pending.id)
        XCTAssertEqual(delivery.lane, .executor)

        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: team.projectID,
            roomID: team.id,
            agentID: agent.id,
            deliveryID: delivery.id,
            triggerMessageID: delivery.messageID,
            rootMessageID: delivery.rootMessageID,
            runID: runID.uuidString.lowercased(),
            hopCount: delivery.hopCount,
            lane: .executor
        )
        var checkpoint = AgentRunCheckpoint(
            scope: LocalAgentGroupChatRun.runtimeScope(for: context),
            messages: [.init(role: .system, content: "system")]
        )
        checkpoint.id = runID
        checkpoint.status = .needsReview
        checkpoint.stopReason = "等待用户确认"
        try await store.saveRun(try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: agent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: 102,
            updatedAtUnixMs: 102
        ))

        let settingsSuite = "local-agent-abandon-executor-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: nativeService,
            services: SchedulerTestServices(),
            settings: .init(suiteName: settingsSuite),
            now: { 200 }
        )
        try await scheduler.abandonDelivery(
            ownerUserID: "alice",
            projectID: team.projectID,
            deliveryID: delivery.id
        )

        let failedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(failedDelivery?.status, .failed)
        let blockedTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(blockedTodo?.status, .blocked)
        XCTAssertEqual(blockedTodo?.blockedReason, "用户已结束这个未完成的本地 Agent Run。")
        let progress = try await store.listAgentTodoProgress(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            limit: 20
        )
        XCTAssertEqual(progress.last?.kind, .blocked)
        XCTAssertEqual(progress.last?.stage, "abandoned")

        let managerWakeValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 201
        )
        let managerWake = try XCTUnwrap(managerWakeValue)
        XCTAssertEqual(managerWake.triggerKind, .todoStatus)
        XCTAssertEqual(managerWake.lane, .manager)
    }

    func testNeedsReviewBlocksTodoButKeepsDeliveryResumable() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-review-executor-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let nativeService = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await nativeService.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "管理并执行任务。",
                modelConfigID: "local-model",
                professionKey: "project_manager"
            )
        )
        let team = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "review-executor-project",
            draft: .init(name: "中断恢复团队"),
            projectManagerAgentID: agent.id
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "review-running-todo",
            draft: .init(
                title: "提交项目文件",
                teamRoomID: team.id,
                creatorAgentID: agent.id
            ),
            nowUnixMs: 100
        )
        let pendingValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        let pending = try XCTUnwrap(pendingValue)
        let deliveryValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 102
        )
        let delivery = try XCTUnwrap(deliveryValue)
        XCTAssertEqual(delivery.id, pending.id)

        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: team.projectID,
            roomID: team.id,
            agentID: agent.id,
            deliveryID: delivery.id,
            triggerMessageID: delivery.messageID,
            rootMessageID: delivery.rootMessageID,
            runID: runID.uuidString.lowercased(),
            hopCount: delivery.hopCount,
            lane: .executor
        )
        var checkpoint = AgentRunCheckpoint(
            scope: LocalAgentGroupChatRun.runtimeScope(for: context),
            messages: [.init(role: .system, content: "system")]
        )
        checkpoint.id = runID
        checkpoint.status = .needsReview
        checkpoint.stopReason = "commit_edit_session 执行结果不明"
        try await store.saveRun(try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: agent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: 102,
            updatedAtUnixMs: 102
        ))

        let settingsSuite = "local-agent-review-executor-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: nativeService,
            services: SchedulerTestServices(),
            settings: .init(suiteName: settingsSuite),
            now: { 200 }
        )
        try await scheduler.suspendTodoForReview(
            store: store,
            ownerUserID: "alice",
            delivery: delivery,
            runID: runID.uuidString.lowercased(),
            detail: "commit_edit_session 执行结果不明"
        )

        let stillRunning = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(stillRunning?.status, .running)
        let blocked = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(blocked?.status, .blocked)
        XCTAssertTrue(blocked?.blockedReason.contains("需检查后重试") == true)
        let progress = try await store.listAgentTodoProgress(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            limit: 20
        )
        XCTAssertEqual(progress.last?.kind, .blocked)
        XCTAssertEqual(progress.last?.stage, "needs_review")
        let requiresHumanRetry = try await store.agentTodoRequiresHumanRetry(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertTrue(requiresHumanRetry)

        do {
            _ = try await store.updateAgentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                todoID: todo.id,
                update: .init(status: .pending, blockedReason: ""),
                nowUnixMs: 201
            )
            XCTFail("Agent 路径不应绕过 needsReview 的 Human 重试入口")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }

        _ = try await store.updateAgentTodoAfterHumanReview(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            update: .init(status: .pending, blockedReason: ""),
            nowUnixMs: 202
        )
        try await scheduler.suspendTodoForReview(
            store: store,
            ownerUserID: "alice",
            delivery: delivery,
            runID: runID.uuidString.lowercased(),
            detail: "commit_edit_session 执行结果不明"
        )
        let repairedPendingState = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(repairedPendingState?.status, .blocked)

        let managerWakeValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 203
        )
        let managerWake = try XCTUnwrap(managerWakeValue)
        XCTAssertEqual(managerWake.triggerKind, .todoStatus)
    }

}
