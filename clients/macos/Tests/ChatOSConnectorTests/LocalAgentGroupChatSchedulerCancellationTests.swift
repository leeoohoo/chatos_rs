import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

extension LocalAgentGroupChatSchedulerTests {
    func testRecoveryDoesNotReenterHumanRetryWhileExecutorIsStillRunning() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-human-retry-recovery-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "恢复执行者",
                rolePrompt: "恢复并完成任务。",
                modelConfigID: "fast-lane-model"
            )
        )
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "human-retry-recovery-project",
            draft: .init(name: "人工恢复团队")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "human-retry-running",
            draft: .init(title: "人工恢复中的任务", teamRoomID: room.id),
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
            projectID: room.projectID,
            roomID: room.id,
            agentID: agent.id,
            deliveryID: delivery.id,
            triggerMessageID: delivery.messageID,
            rootMessageID: delivery.rootMessageID,
            runID: runID.uuidString.lowercased(),
            hopCount: delivery.hopCount,
            lane: .executor
        )
        let interruptedCall = AgentToolCall(
            id: "interrupted-context-read",
            name: LocalAgentChatToolProvider.todoGetContextToolName,
            arguments: "{}"
        )
        var checkpoint = AgentRunCheckpoint(
            scope: LocalAgentGroupChatRun.runtimeScope(for: context),
            messages: [.init(role: .system, content: "system")]
        )
        checkpoint.id = runID
        checkpoint.status = .needsReview
        checkpoint.stopReason = "上次工具结果需要 Human 确认"
        checkpoint.pendingCalls = [interruptedCall]
        checkpoint.inFlightCallID = interruptedCall.id
        let memoryScope = try AgentMemoryScope(
            tenantID: "alice",
            todoID: todo.id,
            runID: runID,
            runtimeScope: checkpoint.scope
        )
        checkpoint.memory = .init(scope: memoryScope, pinnedMessageCount: 1)
        checkpoint.memory?.threadCreated = true
        try await store.saveRun(try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: agent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: 102,
            updatedAtUnixMs: 102
        ))

        let probe = CommunicationFastLaneProbe()
        let settingsSuite = "local-agent-human-retry-recovery-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: CommunicationFastLaneServices(probe: probe),
            settings: .init(suiteName: settingsSuite),
            now: { 300 }
        )
        try await scheduler.suspendTodoForReview(
            store: store,
            ownerUserID: "alice",
            delivery: delivery,
            runID: context.runID,
            detail: checkpoint.stopReason ?? "需要检查"
        )

        let retryTask = Task {
            try await scheduler.retryInterruptedDelivery(
                ownerUserID: "alice",
                projectID: room.projectID,
                deliveryID: delivery.id
            )
        }
        await probe.waitUntilExecutorStarts()

        let recoveryTask = Task {
            try await scheduler.recoverInterruptedRuns(
                store: store,
                ownerUserID: "alice",
                maximumRuns: 2
            )
        }
        try await Task.sleep(for: .milliseconds(100))

        let executorCalls = await probe.executorCalls()
        XCTAssertEqual(executorCalls, 1)
        let executingTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(executingTodo?.status, .inProgress)

        await probe.releaseExecutor()
        let recovered = try await recoveryTask.value
        let retried = try await retryTask.value
        XCTAssertTrue(recovered.isEmpty)
        XCTAssertEqual(retried.outcome, .completed)
        let completedTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        let completedRun = try await store.run(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(completedTodo?.status, .completed)
        XCTAssertEqual(completedRun?.checkpoint.status, .completed)
    }

    func testHumanResolutionRepairsPausedRunWithStillRunningDelivery() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-split-state-recovery-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "状态修复执行者",
                rolePrompt: "恢复并完成任务。",
                modelConfigID: "fast-lane-model"
            )
        )
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "split-state-recovery-project",
            draft: .init(name: "状态修复团队")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "repair-paused-running-delivery",
            draft: .init(title: "修复分裂状态", teamRoomID: room.id),
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
            projectID: room.projectID,
            roomID: room.id,
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
        checkpoint.status = .paused
        checkpoint.stopReason = "连续无进展，已暂停。请检查工具错误或调整设置后继续。"
        checkpoint.noProgressRounds = 8
        let memoryScope = try AgentMemoryScope(
            tenantID: "alice",
            todoID: todo.id,
            runID: runID,
            runtimeScope: checkpoint.scope
        )
        checkpoint.memory = .init(scope: memoryScope, pinnedMessageCount: 1)
        checkpoint.memory?.threadCreated = true
        try await store.saveRun(try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: agent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: 102,
            updatedAtUnixMs: 102
        ))
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            update: .init(
                status: .blocked,
                blockedReason: "旧版本恢复竞态留下的阻塞"
            ),
            nowUnixMs: 103
        )
        // This is the durable state written by the existing "处理阻塞" UI action.
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            update: .init(status: .pending, blockedReason: ""),
            nowUnixMs: 104
        )

        let probe = CommunicationFastLaneProbe()
        let settingsSuite = "local-agent-split-state-recovery-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: CommunicationFastLaneServices(probe: probe),
            settings: .init(suiteName: settingsSuite),
            now: { 300 }
        )
        let recoveryTask = Task {
            try await scheduler.recoverInterruptedRuns(
                store: store,
                ownerUserID: "alice",
                maximumRuns: 2
            )
        }
        await probe.waitUntilExecutorStarts()

        let repairedTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(repairedTodo?.status, .inProgress)

        await probe.releaseExecutor()
        let recovered = try await recoveryTask.value
        XCTAssertEqual(recovered.count, 1)
        XCTAssertEqual(recovered.first?.outcome, .completed)
        let completedTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        let completedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(completedTodo?.status, .completed)
        XCTAssertEqual(completedDelivery?.status, .completed)
    }

}
