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
            settings: .init(suiteName: settingsSuite),
            maximumConcurrentExecutions: 2
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

    func testSchedulerRunsOneAgentAcrossIndependentRoomsConcurrently() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-cross-room-scheduler-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "跨会话 Agent",
                rolePrompt: "独立处理不同会话。",
                modelConfigID: "parallel-model"
            )
        )
        let team = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "cross-room-project",
            draft: .init(name: "跨会话项目")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: team.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let direct = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        let teamPost = try await store.postMessage(
            ownerUserID: "alice",
            roomID: team.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "处理项目会话",
                mentionedAgentIDs: [agent.id]
            ),
            limits: .init()
        )
        let directPost = try await store.postMessage(
            ownerUserID: "alice",
            roomID: direct.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "同时处理私聊会话"
            ),
            limits: .init()
        )

        let probe = SchedulerConcurrencyProbe()
        let settingsSuite = "local-agent-cross-room-scheduler-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: ParallelSchedulerTestServices(probe: probe),
            settings: .init(suiteName: settingsSuite),
            maximumConcurrentExecutions: 2
        )

        let results = try await scheduler.drainCommunications(
            ownerUserID: "alice",
            maximumRuns: 2
        )

        let expectedDeliveryIDs = Set((teamPost.deliveries + directPost.deliveries).map(\.id))
        XCTAssertEqual(Set(results.map(\.deliveryID)), expectedDeliveryIDs)
        XCTAssertTrue(results.allSatisfy { $0.outcome == .completed })
        let maximumConcurrentCalls = await probe.maximumConcurrentCalls()
        XCTAssertGreaterThanOrEqual(maximumConcurrentCalls, 2)
    }

    func testRecoveryRunsIndependentInterruptedConversationsConcurrently() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-parallel-recovery-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        var expectedDeliveryIDs = Set<String>()
        for index in 0..<3 {
            let agent = try await store.createAgent(
                ownerUserID: "alice",
                draft: .init(
                    name: "恢复 Agent \(index)",
                    rolePrompt: "独立恢复不同会话。",
                    modelConfigID: "recovery-parallel-model"
                )
            )
            let room = try await store.openHumanAgentDirect(
                ownerUserID: "alice",
                agentID: agent.id
            )
            let post = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: "alice",
                    content: "恢复独立会话 \(index)"
                ),
                limits: .init()
            )
            let claimedValue = try await store.claimNextDelivery(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                nowUnixMs: Int64(100 + index)
            )
            let delivery = try XCTUnwrap(claimedValue)
            expectedDeliveryIDs.insert(delivery.id)
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
                hopCount: delivery.hopCount
            )
            var checkpoint = AgentRunCheckpoint(
                scope: LocalAgentGroupChatRun.runtimeScope(for: context),
                messages: [
                    .init(role: .system, content: "system"),
                    .init(role: .user, content: post.message.content),
                ]
            )
            checkpoint.id = runID
            checkpoint.status = .ready
            try await store.saveRun(try LocalAgentGroupChatRun(
                id: runID,
                context: context,
                modelConfigID: agent.draft.modelConfigID,
                policy: .init(),
                checkpoint: checkpoint,
                createdAtUnixMs: Int64(100 + index),
                updatedAtUnixMs: Int64(100 + index)
            ))
        }

        let probe = SchedulerConcurrencyProbe()
        let settingsSuite = "local-agent-parallel-recovery-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: RecoveryConcurrencyServices(probe: probe),
            settings: .init(suiteName: settingsSuite),
            maximumConcurrentExecutions: 2,
            now: { 1_000 }
        )

        let results = try await scheduler.recoverInterruptedRuns(
            store: store,
            ownerUserID: "alice",
            maximumRuns: 3
        )

        XCTAssertEqual(Set(results.map(\.deliveryID)), expectedDeliveryIDs)
        XCTAssertTrue(results.allSatisfy { $0.outcome == .completed })
        let maximumConcurrentCalls = await probe.maximumConcurrentCalls()
        XCTAssertEqual(maximumConcurrentCalls, 2)
    }

    func testSchedulerRunsOneAgentsTodosAcrossIndependentTeamsConcurrently() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-cross-team-executors-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "跨项目执行 Agent",
                rolePrompt: "独立处理不同项目任务。",
                modelConfigID: "lane-model"
            )
        )
        var todos: [LocalAgentTodo] = []
        for index in 0..<2 {
            let room = try await store.createRoom(
                ownerUserID: "alice",
                projectID: "cross-team-project-\(index)",
                draft: .init(name: "并行团队 \(index)")
            )
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "执行者")
            )
            todos.append(try await store.createAgentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                requestKey: "cross-team-todo-\(index)",
                draft: .init(title: "并行任务 \(index)", teamRoomID: room.id),
                nowUnixMs: Int64(100 + index)
            ))
        }
        var deliveries: [ProjectAgentDelivery] = []
        for index in 0..<2 {
            let delivery = try await store.startNextReadyAgentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                nowUnixMs: Int64(200 + index)
            )
            deliveries.append(try XCTUnwrap(delivery))
        }
        XCTAssertEqual(Set(deliveries.compactMap(\.todoID)), Set(todos.map(\.id)))

        let probe = SchedulerConcurrencyProbe()
        let settingsSuite = "local-agent-cross-team-executor-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: LaneSchedulerTestServices(probe: probe),
            settings: .init(suiteName: settingsSuite),
            maximumConcurrentExecutions: 2,
            now: { 1_000 }
        )

        let results = try await scheduler.drainAccount(
            ownerUserID: "alice",
            maximumRuns: 2
        )

        XCTAssertEqual(Set(results.map(\.deliveryID)), Set(deliveries.map(\.id)))
        XCTAssertTrue(results.allSatisfy { $0.outcome == .completed })
        let maximumConcurrentCalls = await probe.maximumConcurrentCalls()
        XCTAssertGreaterThanOrEqual(maximumConcurrentCalls, 2)
        for todo in todos {
            let stored = try await store.agentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                todoID: todo.id
            )
            XCTAssertEqual(stored?.status, .completed)
        }
    }

    func testLaterCrossTeamExecutorStartsWhileEarlierAccountDrainIsStillRunning() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-late-cross-team-executor-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "动态并行 Agent",
                rolePrompt: "后到的独立任务不能等待前一个项目。",
                modelConfigID: "cross-team-isolation-model"
            )
        )
        var todos: [LocalAgentTodo] = []
        for index in 0..<2 {
            let room = try await store.createRoom(
                ownerUserID: "alice",
                projectID: "late-cross-team-project-\(index)",
                draft: .init(name: "动态并行团队 \(index)")
            )
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "执行者")
            )
            todos.append(try await store.createAgentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                requestKey: "late-cross-team-todo-\(index)",
                draft: .init(title: "动态并行任务 \(index)", teamRoomID: room.id),
                nowUnixMs: Int64(100 + index)
            ))
        }
        let firstDeliveryValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 200
        )
        let firstDelivery = try XCTUnwrap(firstDeliveryValue)
        let probe = CrossTeamExecutorIsolationProbe()
        let settingsSuite = "local-agent-late-cross-team-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: CrossTeamExecutorIsolationServices(probe: probe),
            settings: .init(suiteName: settingsSuite),
            maximumConcurrentExecutions: 2,
            now: { 1_000 }
        )

        let firstDrain = Task {
            try await scheduler.drainAccount(ownerUserID: "alice", maximumRuns: 1)
        }
        await probe.waitUntilFirstStarts()
        let secondDeliveryValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 201
        )
        let secondDelivery = try XCTUnwrap(secondDeliveryValue)
        let secondDrain = Task {
            try await scheduler.drainAccount(ownerUserID: "alice", maximumRuns: 1)
        }

        var secondStarted = false
        for _ in 0..<100 {
            if await probe.hasSecondStarted() {
                secondStarted = true
                break
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertTrue(secondStarted, "later independent executor must not wait for the first run")
        guard secondStarted else {
            await probe.releaseFirst()
            _ = try await firstDrain.value
            secondDrain.cancel()
            _ = try? await secondDrain.value
            return
        }

        let secondResults = try await secondDrain.value
        XCTAssertEqual(secondResults.map(\.deliveryID), [secondDelivery.id])
        let firstStillRunning = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: firstDelivery.id
        )
        XCTAssertEqual(firstStillRunning?.status, .running)

        await probe.releaseFirst()
        let firstResults = try await firstDrain.value
        XCTAssertEqual(firstResults.map(\.deliveryID), [firstDelivery.id])
        XCTAssertEqual(Set([firstDelivery.todoID, secondDelivery.todoID].compactMap { $0 }), Set(todos.map(\.id)))
    }

    func testSchedulerNeverExecutesOrClaimsBeyondSharedExecutionBudget() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-execution-budget-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "budget-project",
            draft: .init(name: "预算项目群")
        )
        var agents: [LocalAgentProfile] = []
        for index in 0..<4 {
            let agent = try await store.createAgent(
                ownerUserID: "alice",
                draft: .init(
                    name: "Agent \(index)",
                    rolePrompt: "执行预算测试。",
                    modelConfigID: "parallel-model"
                )
            )
            agents.append(agent)
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
                content: "按预算执行",
                mentionedAgentIDs: agents.map(\.id)
            ),
            limits: .init()
        )
        let probe = SchedulerConcurrencyProbe()
        let settingsSuite = "local-agent-execution-budget-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: ParallelSchedulerTestServices(probe: probe),
            settings: .init(suiteName: settingsSuite),
            maximumConcurrentExecutions: 2
        )

        let results = try await scheduler.drainProject(
            ownerUserID: "alice",
            projectID: room.projectID,
            maximumRuns: 64
        )

        XCTAssertEqual(results.count, post.deliveries.count)
        let maximumConcurrentCalls = await probe.maximumConcurrentCalls()
        XCTAssertEqual(maximumConcurrentCalls, 2)
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
            maximumConcurrentExecutions: 2,
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

    func testConcurrentAccountDrainsPreserveSameRoomSerializationWithoutDuplicateClaims() async throws {
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
        var allResults = firstResults + secondResults
        if allResults.count < 2 {
            allResults.append(contentsOf: try await scheduler.drainAccount(
                ownerUserID: "alice",
                maximumRuns: 2 - allResults.count
            ))
        }
        let maximumConcurrentCalls = await probe.maximumConcurrentCalls()

        XCTAssertEqual(allResults.count, 2)
        XCTAssertEqual(Set(allResults.map(\.deliveryID)).count, 2)
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
            maximumConcurrentExecutions: 2,
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
