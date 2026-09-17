import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class LocalAgentGroupChatSchedulerTests: XCTestCase {
    func testDirectConversationInjectsProfessionWithoutProjectType() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-direct-skill-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(databaseURL: folder.appendingPathComponent("chat.db"))
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "研究员",
                rolePrompt: "完成研究任务。",
                modelConfigID: "local-model",
                professionKey: "research_specialist"
            )
        )
        let room = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "开始研究"),
            limits: .init()
        )
        let delivery = try XCTUnwrap(post.deliveries.first)
        let settingsSuite = "local-agent-direct-skill-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let skillLibrary = LocalAgentSkillLibrary(
            fileURL: folder.appendingPathComponent("skill-overrides.json")
        )
        try skillLibrary.updateProfession(
            ownerUserID: "alice",
            key: "research_specialist",
            label: "专项研究员",
            description: "完成专项研究",
            skillMarkdown: "# 当前账户自定义职业规则\n必须给出可核验结论。"
        )
        let scheduler = LocalAgentGroupChatScheduler(
            service: service,
            services: SchedulerTestServices(),
            settings: .init(suiteName: settingsSuite),
            projectTypeKeyProvider: { _, _ in
                XCTFail("私聊不应读取项目类型")
                return "web_application"
            },
            professionProvider: { ownerUserID, key in
                skillLibrary.profession(ownerUserID: ownerUserID, key: key)
            },
            professionCatalogProvider: { ownerUserID in
                skillLibrary.professions(ownerUserID: ownerUserID)
            }
        )
        _ = try await scheduler.drainConversation(ownerUserID: "alice", roomID: room.id)
        let storedRun = try await store.run(ownerUserID: "alice", deliveryID: delivery.id)
        let run = try XCTUnwrap(storedRun)
        let system = run.checkpoint.messages.first?.content ?? ""
        XCTAssertTrue(system.contains(#"name="chatos-profession-research-specialist""#))
        XCTAssertTrue(system.contains("当前账户自定义职业规则"))
        XCTAssertFalse(system.contains("chatos-project-type-"))
    }

    func testSchedulerRunsDeliveryLocallyAndPersistsCompletedRun() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-scheduler-\(UUID().uuidString)")
        let databaseURL = folder.appendingPathComponent("chat.db")
        defer { try? FileManager.default.removeItem(at: folder) }

        let nativeService = NativeAgentGroupChatService(databaseURL: databaseURL)
        let store = try await nativeService.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "客户端工程师",
                description: "实现客户端功能",
                rolePrompt: "先读取群聊，再给出可执行答复。",
                modelConfigID: "local-model",
                thinkingLevel: "high",
                professionKey: "desktop_engineer"
            )
        )
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "project-1",
            draft: .init(name: "项目群", goal: "完成本地多 Agent 协作")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "客户端工程师", responsibility: "实现群聊")
        )
        _ = try await store.setDefaultAgent(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id
        )
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "开始实现"),
            limits: .init()
        )
        let delivery = try XCTUnwrap(post.deliveries.first)

        let settingsSuite = "local-agent-scheduler-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: nativeService,
            services: SchedulerTestServices(expectedThinkingLevel: "high"),
            settings: .init(suiteName: settingsSuite),
            projectTypeKeyProvider: { _, _ in "desktop_application" }
        )
        let results = try await scheduler.drainProject(
            ownerUserID: "alice",
            projectID: "project-1"
        )

        XCTAssertEqual(results.count, 1)
        XCTAssertEqual(results.first?.outcome, .completed)
        let completedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(completedDelivery?.status, .completed)
        let messages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 10
        )
        XCTAssertEqual(messages.map(\.content), ["开始实现", "我已在客户端完成本地调度。"])

        let savedRun = try await store.run(ownerUserID: "alice", deliveryID: delivery.id)
        XCTAssertEqual(savedRun?.checkpoint.status, .completed)
        XCTAssertEqual(savedRun?.context.agentID, agent.id)
        XCTAssertEqual(savedRun?.context.projectID, "project-1")
        XCTAssertNil(savedRun?.checkpoint.memory)
        XCTAssertTrue(savedRun?.events.contains(where: { $0.kind == "memory_unavailable" }) == true)
        let system = savedRun?.checkpoint.messages.first?.content ?? ""
        XCTAssertTrue(system.contains(#"name="chatos-profession-desktop-engineer""#))
        XCTAssertTrue(system.contains(#"name="chatos-project-type-desktop-application""#))
        XCTAssertTrue(system.contains("Desktop Application Playbook") || system.contains("桌面"))
    }

    func testResumeKeepsUnknownWriteInNeedsReviewUntilUserAbandonsIt() async throws {
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
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "project-1",
            draft: .init(name: "项目群")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "实现者")
        )
        _ = try await store.setDefaultAgent(
            ownerUserID: "alice",
            roomID: room.id,
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
            projectID: "project-1",
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
            name: LocalAgentChatToolProvider.sendMessageToolName,
            arguments: #"{"content":"可能已经发送"}"#
        )
        checkpoint.pendingCalls = [call]
        checkpoint.inFlightCallID = call.id
        checkpoint.status = .running
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
            projectID: "project-1",
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
        let runningDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(runningDelivery?.status, .running)
        let listedRuns = try await store.listUnfinishedRuns(
            ownerUserID: "alice",
            projectID: "project-1",
            limit: 10
        )
        XCTAssertEqual(listedRuns.map(\.id), [runID])

        try await scheduler.abandonDelivery(
            ownerUserID: "alice",
            projectID: "project-1",
            deliveryID: delivery.id
        )
        let failedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        let failedRun = try await store.run(ownerUserID: "alice", deliveryID: delivery.id)
        let unfinishedAfterAbandon = try await store.listUnfinishedRuns(
            ownerUserID: "alice",
            projectID: "project-1",
            limit: 10
        )
        XCTAssertEqual(failedDelivery?.status, .failed)
        XCTAssertEqual(failedRun?.checkpoint.status, .failed)
        XCTAssertTrue(unfinishedAfterAbandon.isEmpty)
    }

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
}

private enum SchedulerTestError: Error {
    case memoryOffline
}

private struct SchedulerTestServices: AgentServiceProviding {
    var expectedThinkingLevel: String?

    init(expectedThinkingLevel: String? = nil) {
        self.expectedThinkingLevel = expectedThinkingLevel
    }

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "local-model")
        return SchedulerTestModel()
    }

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy,
        thinkingLevel: String?
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "local-model")
        XCTAssertEqual(thinkingLevel, expectedThinkingLevel)
        return SchedulerTestModel()
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        SchedulerOfflineMemory()
    }
}

private struct SchedulerOfflineMemory: AgentMemoryServicing {
    func ensureThread() async throws { throw SchedulerTestError.memoryOffline }
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws {
        throw SchedulerTestError.memoryOffline
    }
    func compose() async throws -> AgentMemoryContext {
        throw SchedulerTestError.memoryOffline
    }
}

private actor SchedulerTestModel: AgentModelClient {
    private var requestCount = 0

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        XCTAssertTrue(messages.contains {
            $0.role == .system && $0.content.contains("chatos-capability-discovery")
        })
        requestCount += 1
        switch requestCount {
        case 1:
            return .init(
                role: .assistant,
                toolCalls: [.init(id: "read-trigger", name: "chat_get_trigger", arguments: "{}")]
            )
        default:
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "send-reply",
                    name: "chat_send_message",
                    arguments: #"{"content":"我已在客户端完成本地调度。"}"#
                )]
            )
        }
    }
}

private struct ParallelSchedulerTestServices: AgentServiceProviding {
    let probe: SchedulerConcurrencyProbe

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "parallel-model")
        return ParallelSchedulerTestModel(probe: probe)
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        SchedulerOfflineMemory()
    }
}

private actor SchedulerConcurrencyProbe {
    private var activeCalls = 0
    private var maximumCalls = 0

    func enter() {
        activeCalls += 1
        maximumCalls = max(maximumCalls, activeCalls)
    }

    func leave() {
        activeCalls -= 1
    }

    func maximumConcurrentCalls() -> Int {
        maximumCalls
    }
}

private actor ParallelSchedulerTestModel: AgentModelClient {
    private let probe: SchedulerConcurrencyProbe
    private var requestCount = 0

    init(probe: SchedulerConcurrencyProbe) {
        self.probe = probe
    }

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        await probe.enter()
        try await Task.sleep(for: .milliseconds(50))
        await probe.leave()
        requestCount += 1
        if requestCount == 1 {
            return .init(
                role: .assistant,
                toolCalls: [.init(id: "read-unread", name: "chat_read_unread", arguments: "{}")]
            )
        }
        return .init(
            role: .assistant,
            toolCalls: [.init(
                id: "send-reply",
                name: "chat_send_message",
                arguments: #"{"content":"并行 Agent 已完成。"}"#
            )]
        )
    }
}
