import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

extension LocalAgentGroupChatSchedulerTests {
    func testRuntimeCircuitBreakerRejectsBeforeDurableDeliveryClaim() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-worker-admission-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "熔断测试执行者",
                rolePrompt: "执行任务。",
                modelConfigID: "local-model"
            )
        )
        let team = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "worker-admission-project",
            draft: .init(name: "熔断测试团队")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: team.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        _ = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "worker-admission-todo",
            draft: .init(title: "等待 Worker 恢复", teamRoomID: team.id),
            nowUnixMs: 100
        )
        let deliveryValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        let delivery = try XCTUnwrap(deliveryValue)
        let settingsSuite = "local-agent-worker-admission-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let runtime = RejectingAgentWorkerAdmissionRuntime()
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: SchedulerTestServices(),
            settings: .init(suiteName: settingsSuite),
            runtime: runtime,
            now: { 200 }
        )

        do {
            _ = try await scheduler.drainAccount(ownerUserID: "alice", maximumRuns: 1)
            XCTFail("scheduler claimed work while the worker circuit was open")
        } catch let error as AgentWorkerProcessClientError {
            guard case .temporarilyUnavailable = error else { throw error }
        }

        let storedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(storedDelivery?.status, .pending)
        let storedRun = try await store.run(ownerUserID: "alice", deliveryID: delivery.id)
        XCTAssertNil(storedRun)
        let executionCount = await runtime.executionCount()
        XCTAssertEqual(executionCount, 0)
    }

    func testWorkerInterruptionWithInFlightToolRequiresReviewInsteadOfReplay() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-worker-interruption-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "进程中断执行者",
                rolePrompt: "执行任务。",
                modelConfigID: "local-model"
            )
        )
        let team = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "worker-interruption-project",
            draft: .init(name: "进程中断团队")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: team.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "worker-interruption-todo",
            draft: .init(title: "中断任务", teamRoomID: team.id),
            nowUnixMs: 100
        )
        let deliveryValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        let delivery = try XCTUnwrap(deliveryValue)
        let settingsSuite = "local-agent-worker-interruption-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: SchedulerTestServices(),
            settings: .init(suiteName: settingsSuite),
            runtime: InterruptedAgentWorkerRuntime(),
            now: { 200 }
        )

        let results = try await scheduler.drainAccount(ownerUserID: "alice", maximumRuns: 1)

        XCTAssertEqual(results.first?.outcome, .suspended)
        let runValue = try await store.run(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        let run = try XCTUnwrap(runValue)
        XCTAssertEqual(run.checkpoint.status, .needsReview)
        XCTAssertNotNil(run.checkpoint.inFlightCallID)
        let storedTodoValue = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        let storedTodo = try XCTUnwrap(storedTodoValue)
        XCTAssertEqual(storedTodo.status, .blocked)
        XCTAssertTrue(storedTodo.blockedReason.contains("副作用"))
    }

    func testCancelledTodoRejectsExecutorToolBeforeSideEffect() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-cancelled-executor-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let nativeService = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await nativeService.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "可取消执行者",
                rolePrompt: "执行被分配的任务。",
                modelConfigID: "cancellation-model"
            )
        )
        let team = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "cancellation-project",
            draft: .init(name: "取消测试团队")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: team.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "cancel-before-tool",
            draft: .init(title: "即将取消", teamRoomID: team.id),
            nowUnixMs: 100
        )
        let delivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        XCTAssertNotNil(delivery)

        let settingsSuite = "local-agent-cancellation-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: nativeService,
            services: CancellationSchedulerTestServices(
                store: store,
                agentID: agent.id,
                todoID: todo.id
            ),
            settings: .init(suiteName: settingsSuite),
            now: { 200 }
        )
        let results = try await scheduler.drainProject(
            ownerUserID: "alice",
            projectID: team.projectID,
            maximumRuns: 1
        )

        XCTAssertEqual(results.first?.outcome, .completed)
        let storedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: try XCTUnwrap(delivery?.id)
        )
        XCTAssertEqual(storedDelivery?.status, .cancelled)
        let progress = try await store.listAgentTodoProgress(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            limit: 20
        )
        XCTAssertFalse(progress.contains { $0.detail == "不应产生的副作用" })
        let cancelledTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(cancelledTodo?.status, .cancelled)
    }

    func testManagerCancellationActivelyCancelsRunningExecutorTask() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-active-cancellation-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let nativeService = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await nativeService.store()
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "管理并执行任务。",
                modelConfigID: "active-cancellation-model",
                professionKey: "project_manager"
            )
        )
        let team = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "active-cancellation-project",
            draft: .init(name: "主动取消团队"),
            projectManagerAgentID: manager.id
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: manager.id,
            requestKey: "long-running-todo",
            draft: .init(
                title: "长时间执行任务",
                teamRoomID: team.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 100
        )
        let executorDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: manager.id,
            nowUnixMs: 101
        )
        _ = try XCTUnwrap(executorDelivery)
        let direct = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: manager.id
        )
        _ = try await store.postMessage(
            ownerUserID: "alice",
            roomID: direct.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "停止正在执行的任务"),
            limits: .init()
        )

        let settingsSuite = "local-agent-active-cancellation-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: nativeService,
            services: ActiveCancellationSchedulerTestServices(),
            settings: .init(suiteName: settingsSuite),
            now: { 200 }
        )
        let startedAt = Date()
        let results = try await scheduler.drainAccount(ownerUserID: "alice", maximumRuns: 2)

        XCTAssertLessThan(Date().timeIntervalSince(startedAt), 5)
        XCTAssertEqual(results.count, 2)
        XCTAssertTrue(results.contains { $0.outcome == .completed })
        XCTAssertTrue(results.contains { $0.outcome == .suspended })
        let cancelledTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: manager.id,
            todoID: todo.id
        )
        XCTAssertEqual(cancelledTodo?.status, .cancelled)
        let progress = try await store.listAgentTodoProgress(
            ownerUserID: "alice",
            agentID: manager.id,
            todoID: todo.id,
            limit: 20
        )
        XCTAssertTrue(progress.contains {
            $0.kind == .cancelled && $0.stage == "cancelled"
        })
    }

    func testFreshDeliveryPausesInsteadOfRunningWithoutContinuousMemory() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-memory-required-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(name: "连续记忆 Agent", rolePrompt: "保持连续。", modelConfigID: "offline-model")
        )
        let room = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "记住之前的工作继续做"),
            limits: .init()
        )
        let delivery = try XCTUnwrap(post.deliveries.first)
        let settingsSuite = "local-agent-memory-required-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: MemoryOfflineSchedulerServices(),
            settings: .init(suiteName: settingsSuite)
        )

        let results = try await scheduler.drainConversation(
            ownerUserID: "alice",
            roomID: room.id
        )

        XCTAssertEqual(results.first?.outcome, .suspended)
        XCTAssertTrue(results.first?.detail?.contains("连续 Memory") == true)
        let storedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(storedDelivery?.status, .running)
        let storedRun = try await store.run(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        let run = try XCTUnwrap(storedRun)
        XCTAssertEqual(run.checkpoint.status, .paused)
        XCTAssertEqual(run.checkpoint.stopReason, AgentContextError.unavailable.localizedDescription)
        XCTAssertTrue(run.events.contains { $0.kind == "memory_unavailable" })
        let messages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 10
        )
        XCTAssertEqual(messages.map(\.content), ["记住之前的工作继续做"])
    }
}

private actor RejectingAgentWorkerAdmissionRuntime: AgentRuntimeExecuting {
    private var executions = 0

    func checkExecutionAdmission() async throws {
        throw AgentWorkerProcessClientError.temporarilyUnavailable(retryAfterSeconds: 30)
    }

    func executeRun(
        checkpoint: AgentRunCheckpoint,
        scope: String,
        policy: AgentRunPolicy,
        model: any AgentModelClient,
        tools: [AgentToolDefinition],
        execute: @escaping AgentRuntime.Executor,
        completionCheck: @escaping AgentRuntime.CompletionCheck,
        contextProvider: (any AgentContextProviding)?,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> AgentRunCheckpoint {
        executions += 1
        return checkpoint
    }

    func executionCount() -> Int { executions }
}

private struct InterruptedAgentWorkerRuntime: AgentRuntimeExecuting {
    func executeRun(
        checkpoint: AgentRunCheckpoint,
        scope: String,
        policy: AgentRunPolicy,
        model: any AgentModelClient,
        tools: [AgentToolDefinition],
        execute: @escaping AgentRuntime.Executor,
        completionCheck: @escaping AgentRuntime.CompletionCheck,
        contextProvider: (any AgentContextProviding)?,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> AgentRunCheckpoint {
        var interrupted = checkpoint
        let call = AgentToolCall(
            id: "interrupted-write",
            name: tools.first(where: { $0.effect != .readOnly })?.name ?? "unknown_write",
            arguments: "{}"
        )
        interrupted.status = .running
        interrupted.pendingCalls = [call]
        interrupted.inFlightCallID = call.id
        try await record(interrupted, .init(
            kind: "tool_started",
            detail: call.name,
            modelCalls: interrupted.modelCalls
        ))
        throw AgentWorkerProcessClientError.workerFailed("simulated crash")
    }
}
