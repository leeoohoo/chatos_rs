import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

extension LocalAgentGroupChatSchedulerTests {
    func testSchedulerRunsDifferentAgentsConcurrentlyButClaimsOneDeliveryPerAgent() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-parallel-scheduler-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let nativeService = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await nativeService.store()
        let first = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(name: "架构师", rolePrompt: "负责架构。", modelConfigID: "parallel-model")
        )
        let second = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(name: "客户端", rolePrompt: "负责客户端。", modelConfigID: "parallel-model")
        )
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "project-parallel",
            draft: .init(name: "并行项目群")
        )
        for agent in [first, second] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: agent.draft.name)
            )
        }
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "请并行处理",
                mentionedAgentIDs: [first.id, second.id]
            ),
            limits: .init()
        )
        XCTAssertEqual(post.deliveries.count, 2)

        let probe = SchedulerConcurrencyProbe()
        let settingsSuite = "local-agent-parallel-scheduler-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: nativeService,
            services: ParallelSchedulerTestServices(probe: probe),
            settings: .init(suiteName: settingsSuite)
        )
        let results = try await scheduler.drainProject(
            ownerUserID: "alice",
            projectID: "project-parallel",
            maximumRuns: 2
        )

        XCTAssertEqual(results.count, 2)
        XCTAssertEqual(Set(results.map(\.agentID)), Set([first.id, second.id]))
        XCTAssertTrue(results.allSatisfy { $0.outcome == .completed })
        let maximumConcurrentCalls = await probe.maximumConcurrentCalls()
        XCTAssertGreaterThanOrEqual(maximumConcurrentCalls, 2)
        for delivery in post.deliveries {
            let stored = try await store.delivery(
                ownerUserID: "alice",
                deliveryID: delivery.id
            )
            XCTAssertEqual(stored?.status, .completed)
        }
    }

    func testSchedulerRunsOneAgentsManagerAndExecutorLanesConcurrently() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-lane-scheduler-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let nativeService = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await nativeService.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "双线程 Agent",
                rolePrompt: "同时处理通讯与执行。",
                modelConfigID: "lane-model"
            )
        )
        let team = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "lane-project",
            draft: .init(name: "双线程项目")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: team.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let source = try await store.postMessage(
            ownerUserID: "alice",
            roomID: team.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "执行项目任务"),
            limits: .init()
        ).message
        _ = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "lane-todo",
            draft: .init(
                title: "执行项目任务",
                teamRoomID: team.id,
                sourceRoomID: team.id,
                sourceMessageID: source.id,
                executionPlan: .init(builtinCapabilities: [])
            ),
            nowUnixMs: source.createdAtUnixMs + 1
        )
        let executorDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: source.createdAtUnixMs + 2
        )
        XCTAssertNotNil(executorDelivery)
        let direct = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        _ = try await store.postMessage(
            ownerUserID: "alice",
            roomID: direct.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "同时回复这条消息"),
            limits: .init()
        )

        let probe = SchedulerConcurrencyProbe()
        let settingsSuite = "local-agent-lane-scheduler-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: nativeService,
            services: LaneSchedulerTestServices(probe: probe),
            settings: .init(suiteName: settingsSuite),
            now: { source.createdAtUnixMs + 10_000 }
        )
        let results = try await scheduler.drainAccount(
            ownerUserID: "alice",
            maximumRuns: 2
        )

        XCTAssertEqual(results.count, 2)
        XCTAssertTrue(results.allSatisfy { $0.agentID == agent.id && $0.outcome == .completed })
        let maximumConcurrentCalls = await probe.maximumConcurrentCalls()
        XCTAssertGreaterThanOrEqual(maximumConcurrentCalls, 2)
        let allTodos = try await store.listAgentTodos(
            ownerUserID: "alice",
            agentID: agent.id,
            includeTerminal: true
        )
        let completedTodo = allTodos.first
        XCTAssertEqual(completedTodo?.status, .completed)
    }

    func testConcurrentAccountDrainsSerializeAndBothConsumeQueuedWork() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-account-lease-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "串行调度 Agent",
                rolePrompt: "依次处理消息。",
                modelConfigID: "lane-model"
            )
        )
        let direct = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        for content in ["第一条消息", "第二条消息"] {
            _ = try await store.postMessage(
                ownerUserID: "alice",
                roomID: direct.id,
                draft: .init(senderKind: .human, senderID: "alice", content: content),
                limits: .init()
            )
        }
        let probe = SchedulerConcurrencyProbe()
        let settingsSuite = "local-agent-account-lease-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: LaneSchedulerTestServices(probe: probe),
            settings: .init(suiteName: settingsSuite)
        )

        async let first = scheduler.drainAccount(ownerUserID: "alice", maximumRuns: 1)
        async let second = scheduler.drainAccount(ownerUserID: "alice", maximumRuns: 1)
        let (firstResults, secondResults) = try await (first, second)
        let maximumConcurrentCalls = await probe.maximumConcurrentCalls()

        XCTAssertEqual([firstResults.count, secondResults.count], [1, 1])
        XCTAssertEqual(maximumConcurrentCalls, 1)
    }

    func testCommunicationLaneRespondsWhileAccountExecutorDrainIsStillRunning() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-communication-fast-lane-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "双通道 Agent",
                rolePrompt: "执行任务时也要及时回复 Human。",
                modelConfigID: "fast-lane-model"
            )
        )
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "fast-lane-project",
            draft: .init(name: "快速沟通团队")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        _ = try await store.setDefaultAgent(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "long-running-fast-lane-todo",
            draft: .init(title: "长时间任务", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let executorDeliveryValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        let executorDelivery = try XCTUnwrap(executorDeliveryValue)
        let probe = CommunicationFastLaneProbe()
        let settingsSuite = "local-agent-communication-fast-lane-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: CommunicationFastLaneServices(probe: probe),
            settings: .init(suiteName: settingsSuite),
            now: { 200 }
        )

        let executorDrain = Task {
            try await scheduler.drainAccount(ownerUserID: "alice", maximumRuns: 1)
        }
        await probe.waitUntilExecutorStarts()

        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "执行时也请回复我"),
            limits: .init()
        )
        let managerDelivery = try XCTUnwrap(post.deliveries.first)
        let managerResults = try await scheduler.drainCommunications(
            ownerUserID: "alice",
            maximumRuns: 2
        )

        XCTAssertEqual(managerResults.map(\.deliveryID), [managerDelivery.id])
        XCTAssertEqual(managerResults.first?.outcome, .completed)
        let storedManagerDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: managerDelivery.id
        )
        XCTAssertEqual(storedManagerDelivery?.status, .completed)
        let storedExecutorDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: executorDelivery.id
        )
        XCTAssertEqual(storedExecutorDelivery?.status, .running)
        let storedTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(storedTodo?.status, .inProgress)

        await probe.releaseExecutor()
        let executorResults = try await executorDrain.value
        XCTAssertEqual(executorResults.first?.outcome, .completed)
    }

    func testRecoveryDoesNotResumeLiveCommunicationFastLane() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-live-communication-recovery-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "恢复隔离 Agent",
                rolePrompt: "正常通讯运行不能被恢复器重复执行。",
                modelConfigID: "fast-lane-model"
            )
        )
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "live-communication-recovery-project",
            draft: .init(name: "恢复隔离团队")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "沟通者")
        )
        _ = try await store.setDefaultAgent(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id
        )
        _ = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "请及时回复"),
            limits: .init()
        )
        let probe = CommunicationFastLaneProbe(holdsFirstManagerCall: true)
        let settingsSuite = "local-agent-live-communication-recovery-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: CommunicationFastLaneServices(probe: probe),
            settings: .init(suiteName: settingsSuite),
            now: { 300 }
        )

        let communicationDrain = Task {
            try await scheduler.drainCommunication(
                ownerUserID: "alice",
                roomID: room.id,
                maximumRuns: 2
            )
        }
        await probe.waitUntilManagerStarts()

        let recovered = try await scheduler.recoverInterruptedRuns(
            store: store,
            ownerUserID: "alice",
            maximumRuns: 2
        )
        XCTAssertTrue(recovered.isEmpty)
        let callsDuringRecovery = await probe.managerCalls()
        XCTAssertEqual(callsDuringRecovery, 1)

        await probe.releaseManager()
        let results = try await communicationDrain.value
        XCTAssertEqual(results.count, 1)
        XCTAssertEqual(results.first?.outcome, .completed)
        let finalCalls = await probe.managerCalls()
        XCTAssertEqual(finalCalls, 2)
    }

}
