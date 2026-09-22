import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class LocalAgentGroupChatSchedulerTests: XCTestCase {
    func testLocalChangeStreamDeliversDurableInvalidationToMatchingRoom() async throws {
        let service = NativeAgentGroupChatService(
            databaseURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("unused-agent-change-\(UUID().uuidString).db")
        )
        let stream = await service.changes(ownerUserID: "alice", roomID: "room-1")
        let received = Task<NativeAgentGroupChatChange?, Never> {
            for await change in stream { return Optional(change) }
            return nil
        }
        await Task.yield()
        let expected = NativeAgentGroupChatChange(
            ownerUserID: "alice",
            roomID: "room-1",
            agentID: "agent-1",
            runID: UUID(),
            kind: .runUpdated
        )

        await service.publishChange(expected)

        let receivedChange = await received.value
        let change = try XCTUnwrap(receivedChange)
        XCTAssertEqual(change, expected)
    }

    func testAutomaticRecoveryAcceptsInterruptedRunsAndSideEffectFreeTechnicalPauses() {
        var checkpoint = AgentRunCheckpoint(
            scope: "account:alice:agent:test:run:test",
            messages: [.init(role: .system, content: "system")]
        )
        checkpoint.status = .paused
        checkpoint.stopReason = AgentContextError.unavailable.localizedDescription
        XCTAssertTrue(LocalAgentGroupChatScheduler.isAutomaticTriggerRecoveryEligible(checkpoint))

        checkpoint.stopReason = AgentContextError.syncUncertain.localizedDescription
        XCTAssertTrue(LocalAgentGroupChatScheduler.isAutomaticTriggerRecoveryEligible(checkpoint))

        checkpoint.stopReason = AgentContextError.invalidHistory.localizedDescription
        XCTAssertFalse(
            LocalAgentGroupChatScheduler.isAutomaticTriggerRecoveryEligible(checkpoint),
            "A deterministic integrity mismatch must not retry forever on every account drain"
        )

        checkpoint.status = .running
        checkpoint.pendingCalls = [.init(id: "write", name: "chat_send_message", arguments: "{}")]
        checkpoint.inFlightCallID = "write"
        XCTAssertTrue(
            LocalAgentGroupChatScheduler.isAutomaticTriggerRecoveryEligible(checkpoint),
            "AgentRuntime will surface an interrupted side effect as needsReview without replaying it"
        )

        checkpoint.status = .needsReview
        XCTAssertFalse(LocalAgentGroupChatScheduler.isAutomaticTriggerRecoveryEligible(checkpoint))

        checkpoint.status = .paused
        checkpoint.pendingCalls = [.init(id: "write", name: "chat_send_message", arguments: "{}")]
        XCTAssertFalse(LocalAgentGroupChatScheduler.isAutomaticTriggerRecoveryEligible(checkpoint))

        checkpoint.pendingCalls = []
        checkpoint.inFlightCallID = "write"
        XCTAssertFalse(LocalAgentGroupChatScheduler.isAutomaticTriggerRecoveryEligible(checkpoint))

        checkpoint.inFlightCallID = nil
        checkpoint.stopReason = "用户已暂停"
        XCTAssertFalse(LocalAgentGroupChatScheduler.isAutomaticTriggerRecoveryEligible(checkpoint))
    }

    func testDirectConversationInjectsSelectedProfessionLanguageWithoutProjectType() async throws {
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
        try skillLibrary.updateProfessionBilingual(
            ownerUserID: "alice",
            key: "research_specialist",
            label: "专项研究员",
            description: "完成专项研究",
            skillMarkdown: "# 当前账户自定义职业规则\n必须给出可核验结论。",
            labelEN: "Specialist Researcher",
            descriptionEN: "Conduct focused research",
            skillMarkdownEN: "# Account-specific research role\nProvide verifiable conclusions."
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
            },
            contextLanguageProvider: { _ in .english }
        )
        _ = try await scheduler.drainConversation(ownerUserID: "alice", roomID: room.id)
        let storedRun = try await store.run(ownerUserID: "alice", deliveryID: delivery.id)
        let run = try XCTUnwrap(storedRun)
        let system = run.checkpoint.messages.first?.content ?? ""
        XCTAssertTrue(system.contains(#"name="chatos-profession-research-specialist""#))
        XCTAssertTrue(system.contains("Account-specific research role"))
        XCTAssertFalse(system.contains("当前账户自定义职业规则"))
        XCTAssertFalse(system.contains("chatos-project-type-"))
        XCTAssertTrue(system.contains(#"name="chatos-compact-communication""#))
        XCTAssertTrue(system.contains("Lead with the conclusion"))
        let communicationSkill = try XCTUnwrap(run.checkpoint.instructionBundleItems.first)
        XCTAssertEqual(communicationSkill.name, "chatos-compact-communication")
        XCTAssertEqual(communicationSkill.version, 2)
        XCTAssertEqual(communicationSkill.language, ChatOSLanguage.english.rawValue)
        XCTAssertEqual(communicationSkill.audience, "manager")
        XCTAssertEqual(communicationSkill.contentSHA256.count, 64)
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
        XCTAssertNotNil(savedRun?.checkpoint.memory)
        XCTAssertFalse(savedRun?.events.contains(where: { $0.kind == "memory_unavailable" }) == true)
        let system = savedRun?.checkpoint.messages.first?.content ?? ""
        XCTAssertTrue(system.contains(#"name="chatos-profession-desktop-engineer""#))
        XCTAssertTrue(system.contains(#"name="chatos-project-type-desktop-application""#))
        XCTAssertTrue(system.contains("Desktop Application Playbook") || system.contains("桌面"))
        XCTAssertTrue(system.contains(#"name="chatos-compact-communication""#))
        XCTAssertEqual(
            savedRun?.checkpoint.instructionBundleItems.first?.audience,
            "manager"
        )
        XCTAssertTrue(savedRun?.checkpoint.messages.dropFirst().first?.content.contains("开始实现") == true)
    }

    func testFailedTodoRetryResumesItsDurableRunAndCompletes() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-todo-retry-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let nativeService = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let store = try await nativeService.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "重试执行者",
                rolePrompt: "完成被分配的任务。",
                modelConfigID: "todo-retry-model"
            )
        )
        let team = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "todo-retry-project",
            draft: .init(name: "重试测试团队")
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
            requestKey: "scheduler-todo-retry",
            draft: .init(title: "恢复失败的任务", teamRoomID: team.id),
            nowUnixMs: 100
        )
        let firstPendingValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        let firstPending = try XCTUnwrap(firstPendingValue)
        let model = TodoRetrySchedulerTestModel()
        let settingsSuite = "local-agent-todo-retry-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: nativeService,
            services: TodoRetrySchedulerTestServices(model: model),
            settings: .init(suiteName: settingsSuite),
            now: { 200 }
        )

        let firstResults = try await scheduler.drainProject(
            ownerUserID: "alice",
            projectID: team.projectID,
            maximumRuns: 1
        )
        XCTAssertEqual(firstResults.first?.outcome, .failed)
        let failedRunValue = try await store.run(
            ownerUserID: "alice",
            deliveryID: firstPending.id
        )
        let failedRun = try XCTUnwrap(failedRunValue)
        XCTAssertEqual(failedRun.checkpoint.status, .failed)
        XCTAssertEqual(failedRun.checkpoint.modelCalls, 1)
        let failedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: firstPending.id
        )
        XCTAssertEqual(failedDelivery?.status, .failed)
        let blockedTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(blockedTodo?.status, .blocked)

        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            update: .init(status: .pending),
            nowUnixMs: 201
        )
        let retriedPendingValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 202
        )
        let retriedPending = try XCTUnwrap(retriedPendingValue)
        XCTAssertEqual(retriedPending.id, firstPending.id)

        let secondResults = try await scheduler.drainProject(
            ownerUserID: "alice",
            projectID: team.projectID,
            maximumRuns: 1
        )
        XCTAssertEqual(secondResults.first?.outcome, .completed)
        let completedRunValue = try await store.run(
            ownerUserID: "alice",
            deliveryID: firstPending.id
        )
        let completedRun = try XCTUnwrap(completedRunValue)
        XCTAssertEqual(completedRun.id, failedRun.id)
        XCTAssertEqual(completedRun.checkpoint.status, .completed)
        XCTAssertEqual(completedRun.checkpoint.modelCalls, 2)
        let completedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: firstPending.id
        )
        XCTAssertEqual(completedDelivery?.status, .completed)
        XCTAssertEqual(completedDelivery?.attempt, 2)
        let completedTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(completedTodo?.status, .completed)
    }

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
        let runningDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(runningDelivery?.status, .running)
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

private enum SchedulerTestError: Error {
    case memoryOffline
}

private struct MemoryOfflineSchedulerServices: AgentServiceProviding {
    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTFail("Memory 未连接时不应调用模型")
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
        SchedulerTestMemory()
    }
}

private struct SchedulerTestMemory: AgentMemoryServicing {
    func ensureThread() async throws {}
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws {}
    func compose() async throws -> AgentMemoryContext {
        .init(blocks: [], recentRecords: [])
    }
}

private struct TodoRetrySchedulerTestServices: AgentServiceProviding {
    let model: TodoRetrySchedulerTestModel

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "todo-retry-model")
        return model
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        SchedulerTestMemory()
    }
}

private actor TodoRetrySchedulerTestModel: AgentModelClient {
    private var requestCount = 0

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        requestCount += 1
        if requestCount == 1 {
            throw AgentRuntimeError.invalidResponse
        }
        XCTAssertTrue(tools.contains {
            $0.name == LocalAgentChatToolProvider.todoCompleteToolName
        })
        return .init(
            role: .assistant,
            toolCalls: [.init(
                id: "complete-retried-todo",
                name: LocalAgentChatToolProvider.todoCompleteToolName,
                arguments: #"{"summary":"恢复原 Run 后完成。"}"#
            )]
        )
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
        XCTAssertTrue(messages.contains {
            $0.role == .user && $0.content.contains("current_trigger_json")
        })
        requestCount += 1
        switch requestCount {
        case 1:
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "send-reply",
                    name: LocalAgentChatToolProvider.sendMessageToolName,
                    arguments: #"{"content":"我已在客户端完成本地调度。"}"#
                )]
            )
        case 2:
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "read-schedule",
                    name: LocalAgentChatToolProvider.todoScheduleStateToolName,
                    arguments: "{}"
                )]
            )
        default:
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "complete-cycle",
                    name: LocalAgentChatToolProvider.completeManagerCycleToolName,
                    arguments: "{}"
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
        SchedulerTestMemory()
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
        if requestCount == 2 {
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "send-reply",
                    name: LocalAgentChatToolProvider.sendMessageToolName,
                    arguments: #"{"content":"并行 Agent 已完成。"}"#
                )]
            )
        }
        return .init(
            role: .assistant,
            toolCalls: [.init(
                id: "complete-cycle",
                name: LocalAgentChatToolProvider.completeManagerCycleToolName,
                arguments: "{}"
            )]
        )
    }
}

private struct LaneSchedulerTestServices: AgentServiceProviding {
    let probe: SchedulerConcurrencyProbe

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "lane-model")
        return LaneSchedulerTestModel(probe: probe)
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        SchedulerTestMemory()
    }
}

private actor LaneSchedulerTestModel: AgentModelClient {
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
        try await Task.sleep(for: .milliseconds(75))
        await probe.leave()
        if tools.contains(where: { $0.name == LocalAgentChatToolProvider.todoCompleteToolName }) {
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "complete-todo",
                    name: LocalAgentChatToolProvider.todoCompleteToolName,
                    arguments: #"{"summary":"执行线程已完成。"}"#
                )]
            )
        }
        requestCount += 1
        if requestCount > 1 {
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "complete-manager-cycle",
                    name: LocalAgentChatToolProvider.completeManagerCycleToolName,
                    arguments: "{}"
                )]
            )
        }
        return .init(
            role: .assistant,
            toolCalls: [.init(
                id: "reply-manager",
                name: LocalAgentChatToolProvider.sendMessageToolName,
                arguments: #"{"content":"通讯线程已回复。"}"#
            )]
        )
    }
}

private struct CancellationSchedulerTestServices: AgentServiceProviding {
    let store: SQLiteAgentGroupChatStore
    let agentID: String
    let todoID: String

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "cancellation-model")
        return CancellationSchedulerTestModel(store: store, agentID: agentID, todoID: todoID)
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        SchedulerTestMemory()
    }
}

private actor CancellationSchedulerTestModel: AgentModelClient {
    let store: SQLiteAgentGroupChatStore
    let agentID: String
    let todoID: String

    init(store: SQLiteAgentGroupChatStore, agentID: String, todoID: String) {
        self.store = store
        self.agentID = agentID
        self.todoID = todoID
    }

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agentID,
            todoID: todoID,
            update: .init(status: .cancelled),
            nowUnixMs: 201
        )
        return .init(
            role: .assistant,
            toolCalls: [.init(
                id: "rejected-progress",
                name: LocalAgentChatToolProvider.todoProgressAppendToolName,
                arguments: #"{"detail":"不应产生的副作用"}"#
            )]
        )
    }
}

private struct ActiveCancellationSchedulerTestServices: AgentServiceProviding {
    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "active-cancellation-model")
        return ActiveCancellationSchedulerTestModel()
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        SchedulerTestMemory()
    }
}

private actor ActiveCancellationSchedulerTestModel: AgentModelClient {
    private var requestCount = 0

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        if tools.contains(where: { $0.name == LocalAgentChatToolProvider.todoCompleteToolName }) {
            try await Task.sleep(for: .seconds(30))
            return .init(role: .assistant, content: "不应自然结束")
        }
        requestCount += 1
        switch requestCount {
        case 1:
            return .init(role: .assistant, toolCalls: [.init(
                id: "list-todos-before-cancel",
                name: LocalAgentChatToolProvider.todoListToolName,
                arguments: "{}"
            )])
        case 2:
            let toolContent = messages.last(where: { $0.role == .tool })?.content ?? "[]"
            let data = Data(toolContent.utf8)
            let values = try JSONSerialization.jsonObject(with: data) as? [[String: Any]]
            let todoReference = try XCTUnwrap(values?.first?["todo_ref"] as? String)
            let arguments = try XCTUnwrap(String(
                data: JSONSerialization.data(withJSONObject: [
                    "todo_ref": todoReference,
                    "status": "cancelled",
                ]),
                encoding: .utf8
            ))
            return .init(role: .assistant, toolCalls: [.init(
                id: "cancel-running-todo",
                name: LocalAgentChatToolProvider.todoUpdateToolName,
                arguments: arguments
            )])
        default:
            return .init(role: .assistant, toolCalls: [.init(
                id: "complete-after-cancel",
                name: LocalAgentChatToolProvider.completeManagerCycleToolName,
                arguments: "{}"
            )])
        }
    }
}
