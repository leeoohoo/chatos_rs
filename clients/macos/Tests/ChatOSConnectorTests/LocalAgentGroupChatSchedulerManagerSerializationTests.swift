import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

extension LocalAgentGroupChatSchedulerTests {
    private func managerFixture(_ folder: URL) async throws -> (
        NativeAgentGroupChatService, LocalAgentProfile, ProjectAgentRoom, ProjectAgentRoom
    ) {
        let service = NativeAgentGroupChatService(databaseURL: folder.appendingPathComponent("chat.db"))
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(name: "单沟通层", rolePrompt: "处理全部会话消息。", modelConfigID: "fast-lane-model")
        )
        let team = try await store.createRoom(
            ownerUserID: "alice", projectID: "manager-serial", draft: .init(name: "团队")
        )
        _ = try await store.addMember(
            ownerUserID: "alice", roomID: team.id, agentID: agent.id, draft: .init(role: "沟通者")
        )
        let direct = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)
        return (service, agent, team, direct)
    }

    func testRunningManagerReceivesCrossRoomMessagesWithoutStartingAnotherManager() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("manager-inbox-\(UUID())")
        defer { try? FileManager.default.removeItem(at: folder) }
        let (service, agent, team, direct) = try await managerFixture(folder)
        let store = try await service.store()
        _ = try await store.postMessage(
            ownerUserID: "alice", roomID: team.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "开始处理", mentionedAgentIDs: [agent.id]), limits: .init()
        )
        let probe = CommunicationFastLaneProbe(holdsFirstManagerCall: true)
        let suite = "manager-inbox-\(UUID())"
        defer { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service, services: CommunicationFastLaneServices(probe: probe),
            settings: .init(suiteName: suite), maximumConcurrentExecutions: 3
        )
        let first = Task { try await scheduler.drainCommunication(ownerUserID: "alice", roomID: team.id, maximumRuns: 1) }
        await probe.waitUntilManagerStarts() // The current run has already read its inbox.
        let arrival = try await store.postMessage(
            ownerUserID: "alice", roomID: direct.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "运行中发来的新消息"), limits: .init()
        )
        let blocked = try await scheduler.drainCommunication(ownerUserID: "alice", roomID: direct.id, maximumRuns: 1)
        let callsWhileBlocked = await probe.managerCalls()
        let unreadWhileRunning = try await store.listAllUnreadMessages(ownerUserID: "alice", agentID: agent.id)
        let pending = try await store.delivery(ownerUserID: "alice", deliveryID: XCTUnwrap(arrival.deliveries.first).id)
        await probe.releaseManager()
        let firstResults = try await first.value

        XCTAssertTrue(blocked.isEmpty)
        XCTAssertEqual(callsWhileBlocked, 1)
        XCTAssertEqual(pending?.status, .pending)
        XCTAssertTrue(unreadWhileRunning.flatMap(\.messages).contains { $0.id == arrival.message.id })
        XCTAssertEqual(firstResults.first?.outcome, .completed)
        // Finishing an older inbox snapshot must not acknowledge a later arrival.
        let unreadAfterCompletion = try await store.listAllUnreadMessages(ownerUserID: "alice", agentID: agent.id)
        XCTAssertTrue(unreadAfterCompletion.flatMap(\.messages).contains { $0.id == arrival.message.id })
        let next = try await scheduler.drainCommunications(ownerUserID: "alice", maximumRuns: 1)
        XCTAssertEqual(next.first?.deliveryID, arrival.deliveries.first?.id)
        XCTAssertEqual(next.first?.outcome, .completed)
        let unreadAfterNext = try await store.listAllUnreadMessages(ownerUserID: "alice", agentID: agent.id)
        XCTAssertFalse(unreadAfterNext.flatMap(\.messages).contains { $0.id == arrival.message.id })
    }

    func testManualAndAutomaticRecoveryCannotStartSecondManagerOrClearReviewMarker() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("manager-recovery-\(UUID())")
        defer { try? FileManager.default.removeItem(at: folder) }
        let (service, agent, team, direct) = try await managerFixture(folder)
        let store = try await service.store()
        let oldPost = try await store.postMessage(
            ownerUserID: "alice", roomID: direct.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "原先暂停的私聊"), limits: .init()
        )
        let claimed = try await store.claimNextDelivery(ownerUserID: "alice", agentID: agent.id, nowUnixMs: 100)
        let oldDelivery = try XCTUnwrap(claimed)
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice", projectID: direct.projectID, roomID: direct.id, agentID: agent.id,
            deliveryID: oldDelivery.id, triggerMessageID: oldPost.message.id, rootMessageID: oldPost.message.rootMessageID,
            runID: runID.uuidString.lowercased(), hopCount: 0
        )
        var checkpoint = AgentRunCheckpoint(scope: LocalAgentGroupChatRun.runtimeScope(for: context), messages: [
            .init(role: .system, content: "system"), .init(role: .user, content: "恢复原运行"),
        ])
        checkpoint.id = runID
        var saved = try LocalAgentGroupChatRun(
            id: runID, context: context, modelConfigID: "fast-lane-model", policy: .init(), checkpoint: checkpoint,
            createdAtUnixMs: 100, updatedAtUnixMs: 100
        )
        try await store.saveRun(saved)
        _ = try await store.failDelivery(ownerUserID: "alice", deliveryID: oldDelivery.id, error: "fixture pause", nowUnixMs: 101)
        _ = try await store.postMessage(
            ownerUserID: "alice", roomID: team.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "当前沟通", mentionedAgentIDs: [agent.id]), limits: .init()
        )
        let probe = CommunicationFastLaneProbe(holdsFirstManagerCall: true)
        let suite = "manager-recovery-\(UUID())"
        defer { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let services = CommunicationFastLaneServices(probe: probe)
        let scheduler = LocalAgentGroupChatScheduler(service: service, services: services, settings: .init(suiteName: suite), maximumConcurrentExecutions: 3)
        // A second scheduler facade must share ownership with the original one.
        let otherScheduler = LocalAgentGroupChatScheduler(service: service, services: services, settings: .init(suiteName: suite), maximumConcurrentExecutions: 3)
        let current = Task { try await scheduler.drainCommunication(ownerUserID: "alice", roomID: team.id, maximumRuns: 1) }
        await probe.waitUntilManagerStarts()
        // Recreate two durable running records left by the previous cross-room implementation.
        _ = try await store.reactivateManagerDeliveryForReview(ownerUserID: "alice", deliveryID: oldDelivery.id, nowUnixMs: 102)
        let manual = try await otherScheduler.resumeDelivery(ownerUserID: "alice", projectID: direct.projectID, deliveryID: oldDelivery.id)
        let recovered = try await otherScheduler.recoverInterruptedRuns(store: store, ownerUserID: "alice", maximumRuns: 3)
        let unchanged = try await store.run(ownerUserID: "alice", deliveryID: oldDelivery.id)
        let uncertain = AgentToolCall(id: "uncertain-send", name: LocalAgentChatToolProvider.sendMessageToolName, arguments: #"{"content":"原步骤"}"#)
        saved.checkpoint.messages.append(.init(role: .assistant, toolCalls: [uncertain]))
        saved.checkpoint.pendingCalls = [uncertain]
        saved.checkpoint.inFlightCallID = uncertain.id
        saved.checkpoint.status = .needsReview
        try await store.saveRun(saved)
        let review = try await otherScheduler.retryInterruptedDelivery(ownerUserID: "alice", projectID: direct.projectID, deliveryID: oldDelivery.id)
        let stillReviewed = try await store.run(ownerUserID: "alice", deliveryID: oldDelivery.id)
        let calls = await probe.managerCalls()
        await probe.releaseManager()
        _ = try await current.value

        XCTAssertEqual(manual.outcome, .suspended)
        XCTAssertTrue(recovered.isEmpty)
        XCTAssertEqual(unchanged?.checkpoint, checkpoint)
        XCTAssertEqual(review.outcome, .suspended)
        XCTAssertEqual(stillReviewed?.checkpoint.inFlightCallID, uncertain.id)
        XCTAssertEqual(stillReviewed?.checkpoint.status, .needsReview)
        XCTAssertEqual(calls, 1)
        XCTAssertTrue(stillReviewed?.events.isEmpty == true)
    }

    func testIndependentDatabaseConnectionsCannotClaimTwoManagersAcrossRooms() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("manager-atomic-\(UUID())")
        defer { try? FileManager.default.removeItem(at: folder) }
        let (service, agent, team, direct) = try await managerFixture(folder)
        let firstStore = try await service.store()
        let secondStore = try SQLiteAgentGroupChatStore(databaseURL: folder.appendingPathComponent("chat.db"))
        for room in [team, direct] {
            _ = try await firstStore.postMessage(
                ownerUserID: "alice", roomID: room.id,
                draft: .init(senderKind: .human, senderID: "alice", content: "同时唤醒", mentionedAgentIDs: [agent.id]), limits: .init()
            )
        }
        async let teamClaim = firstStore.claimNextDelivery(ownerUserID: "alice", roomID: team.id, agentID: agent.id, lane: .manager, nowUnixMs: 100)
        async let directClaim = secondStore.claimNextDelivery(ownerUserID: "alice", roomID: direct.id, agentID: agent.id, lane: .manager, nowUnixMs: 100)
        let claims = try await [teamClaim, directClaim].compactMap { $0 }
        XCTAssertEqual(claims.count, 1)
        let held = try XCTUnwrap(claims.first)
        _ = try await firstStore.failDelivery(ownerUserID: "alice", deliveryID: held.id, error: "finished fixture", nowUnixMs: 101)
        let next = try await secondStore.claimNextDelivery(ownerUserID: "alice", agentID: agent.id, lane: .manager, nowUnixMs: 102)
        XCTAssertNotNil(next)
        XCTAssertNotEqual(next?.roomID, held.roomID)
    }

    func testManagerHandoffReturnsUnstartedClaimToQueue() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("manager-handoff-\(UUID())")
        defer { try? FileManager.default.removeItem(at: folder) }
        let (service, agent, _, direct) = try await managerFixture(folder)
        let store = try await service.store()
        let arrival = try await store.postMessage(
            ownerUserID: "alice", roomID: direct.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "紧接着到达的消息"), limits: .init()
        )
        let ownership = await service.activeDeliveryRegistry.acquire(deliveryID: "finishing-previous-cycle", ownerUserID: "alice", agentID: agent.id, lane: .manager)
        XCTAssertEqual(ownership, .acquired)
        let suite = "manager-handoff-\(UUID())"
        defer { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let scheduler = LocalAgentGroupChatScheduler(service: service, services: CommunicationFastLaneServices(probe: CommunicationFastLaneProbe()), settings: .init(suiteName: suite))
        let blocked = try await scheduler.drainCommunications(ownerUserID: "alice", maximumRuns: 1)
        let pending = try await store.delivery(ownerUserID: "alice", deliveryID: XCTUnwrap(arrival.deliveries.first).id)
        let noRun = try await store.run(ownerUserID: "alice", deliveryID: XCTUnwrap(arrival.deliveries.first).id)
        await service.activeDeliveryRegistry.unregister(deliveryID: "finishing-previous-cycle")
        let next = try await scheduler.drainCommunications(ownerUserID: "alice", maximumRuns: 1)
        XCTAssertTrue(blocked.isEmpty)
        XCTAssertEqual(pending?.status, .pending)
        XCTAssertNil(noRun)
        XCTAssertEqual(next.first?.outcome, .completed)
    }
}
