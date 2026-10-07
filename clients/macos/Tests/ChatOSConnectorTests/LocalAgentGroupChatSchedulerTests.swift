import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class LocalAgentGroupChatSchedulerTests: XCTestCase {
    func testLocalChangeStreamPreservesRoomUpdateAcrossCheckpointOverflow() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(
            databaseURL: folder.appendingPathComponent("chat.db")
        )
        let stream = await service.changes(ownerUserID: "user-1", roomID: "room-1")
        await service.publishChange(.init(
            ownerUserID: "user-1",
            roomID: "room-1",
            kind: .roomUpdated
        ))
        for _ in 0..<128 {
            await service.publishChange(.init(
                ownerUserID: "user-1",
                roomID: "room-1",
                runID: UUID(),
                kind: .runUpdated
            ))
        }

        var iterator = stream.makeAsyncIterator()
        var received: [NativeAgentGroupChatChange] = []
        for _ in 0..<64 {
            if let change = await iterator.next() {
                received.append(change)
            }
        }
        XCTAssertEqual(received.count, 64)
        XCTAssertTrue(received.contains { $0.kind == .roomUpdated })
    }

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
        XCTAssertTrue(
            LocalAgentGroupChatScheduler.isAutomaticTriggerRecoveryEligible(checkpoint),
            "A claimed delivery interrupted before its first model call must resume from ready"
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
        XCTAssertFalse(system.contains("Account-specific research role"))
        XCTAssertEqual(run.progressiveSkillSnapshot?.skills.count, 1)
        XCTAssertTrue(
            run.progressiveSkillSnapshot?.skills.first?.instructions.contains(
                "Account-specific research role"
            ) == true
        )
        XCTAssertTrue(system.contains("agent_skill_activate"))
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
        let changes = await nativeService.changes(ownerUserID: "alice", roomID: room.id)
        let roomUpdateReceived = expectation(description: "delivery completion invalidates room")
        let observer = Task {
            for await change in changes where change.kind == .roomUpdated {
                roomUpdateReceived.fulfill()
                return
            }
        }
        defer { observer.cancel() }
        let results = try await scheduler.drainProject(
            ownerUserID: "alice",
            projectID: "project-1"
        )
        await fulfillment(of: [roomUpdateReceived], timeout: 1)

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
        XCTAssertFalse(system.contains("AS-project_type-desktop_application"))
        XCTAssertEqual(savedRun?.progressiveSkillSnapshot?.skills.map(\.name), [
            "chatos-profession-desktop-engineer",
        ])
        XCTAssertFalse(system.contains("Desktop Application Playbook"))
        XCTAssertTrue(system.contains(#"name="chatos-compact-communication""#))
        XCTAssertEqual(savedRun?.checkpoint.instructionBundleItems.first?.audience, "manager")
        let wakeMessage = savedRun?.checkpoint.messages.dropFirst().first?.content ?? ""
        XCTAssertTrue(wakeMessage.contains("读取全部未读"))
        XCTAssertFalse(wakeMessage.contains("开始实现"))
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

}
