import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import XCTest

final class SQLiteAgentGroupChatStoreTests: XCTestCase {
    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-group-chat-\(UUID().uuidString)")
            .appendingPathComponent("group-chat.db")
    }

    private func makeAgent(
        _ store: SQLiteAgentGroupChatStore,
        owner: String = "alice",
        name: String
    ) async throws -> LocalAgentProfile {
        try await store.createAgent(
            ownerUserID: owner,
            draft: .init(
                name: name,
                rolePrompt: "你是\(name)，只处理当前项目中明确交给你的工作。",
                modelConfigID: "model-1"
            )
        )
    }

    private func makeRoom(
        _ store: SQLiteAgentGroupChatStore,
        owner: String = "alice",
        projectID: String = "project-1"
    ) async throws -> ProjectAgentRoom {
        try await store.createRoom(
            ownerUserID: owner,
            projectID: projectID,
            draft: .init(name: "项目群聊", goal: "协作完成项目")
        )
    }

    func testMentionCreatesDurableDeliveryWithStableAgentIdentity() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "架构师")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "架构师")
        )

        let posted = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "@架构师 请审查设计",
                mentionedAgentIDs: [agent.id]
            ),
            limits: .init()
        )
        XCTAssertEqual(posted.message.mentionedAgentIDs, [agent.id])
        XCTAssertEqual(posted.deliveries.count, 1)
        XCTAssertEqual(posted.deliveries.first?.targetAgentID, agent.id)
        XCTAssertEqual(posted.deliveries.first?.triggerKind, .mention)

        let reopened = try SQLiteAgentGroupChatStore(databaseURL: url)
        let messages = try await reopened.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 20
        )
        XCTAssertEqual(messages, [posted.message])
        let claimed = try await reopened.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: posted.message.createdAtUnixMs + 1
        )
        XCTAssertEqual(claimed?.id, posted.deliveries.first?.id)
        XCTAssertEqual(claimed?.status, .running)
        XCTAssertEqual(claimed?.attempt, 1)
    }

    func testHumanMessageWithoutMentionRoutesOnlyToDefaultAgent() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let first = try await makeAgent(store, name: "默认成员")
        let second = try await makeAgent(store, name: "其他成员")
        let room = try await makeRoom(store)
        for agent in [first, second] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: agent.draft.name)
            )
        }
        _ = try await store.setDefaultAgent(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: first.id
        )

        let posted = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "请看一下"),
            limits: .init()
        )
        XCTAssertEqual(posted.deliveries.map(\.targetAgentID), [first.id])
        XCTAssertEqual(posted.deliveries.first?.triggerKind, .defaultAgent)
        let otherClaim = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: second.id,
            nowUnixMs: posted.message.createdAtUnixMs + 1
        )
        XCTAssertNil(otherClaim)
    }

    func testOnlyOneActiveDeliveryCanBeClaimedPerAgent() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "客户端")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "客户端")
        )
        let first = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "第一条",
                mentionedAgentIDs: [agent.id]
            ),
            limits: .init()
        )
        _ = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "第二条",
                mentionedAgentIDs: [agent.id]
            ),
            limits: .init()
        )
        let firstClaim = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: first.message.createdAtUnixMs + 1
        )
        let claimed = try XCTUnwrap(firstClaim)
        let duplicateClaim = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: first.message.createdAtUnixMs + 2
        )
        XCTAssertNil(duplicateClaim)

        let response = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .agent,
                senderID: agent.id,
                content: "第一条已处理",
                replyToMessageID: first.message.id,
                rootMessageID: first.message.rootMessageID,
                hopCount: 1
            ),
            limits: .init()
        )
        let completed = try await store.completeDelivery(
            ownerUserID: "alice",
            deliveryID: claimed.id,
            responseMessageID: response.message.id,
            nowUnixMs: first.message.createdAtUnixMs + 3
        )
        XCTAssertEqual(completed.status, .completed)
        XCTAssertEqual(completed.responseMessageID, response.message.id)
        let nextClaim = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: first.message.createdAtUnixMs + 4
        )
        XCTAssertNotNil(nextClaim)
    }

    func testAgentCannotImpersonateHumanOrMentionNonMember() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let member = try await makeAgent(store, name: "成员")
        let outsider = try await makeAgent(store, name: "外部 Agent")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: member.id,
            draft: .init(role: "成员")
        )

        do {
            _ = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: member.id,
                    content: "伪造用户消息"
                ),
                limits: .init()
            )
            XCTFail("Impersonation was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }

        do {
            _ = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: "alice",
                    content: "@外部 Agent",
                    mentionedAgentIDs: [outsider.id]
                ),
                limits: .init()
            )
            XCTFail("Non-member mention was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .notMember)
        }
        let messages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 20
        )
        XCTAssertTrue(messages.isEmpty)
    }

    func testRoutingBudgetKeepsMessageButStopsAgentWakeup() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "成员")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "成员")
        )
        let posted = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "超过深度仍应保存",
                mentionedAgentIDs: [agent.id],
                hopCount: 5
            ),
            limits: .init(maximumHopCount: 4, maximumAgentRunsPerRootMessage: 12)
        )
        XCTAssertTrue(posted.deliveries.isEmpty)
        XCTAssertNotNil(posted.routingStopReason)
        let messages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 20
        )
        XCTAssertEqual(messages.map(\.id), [posted.message.id])
    }

    func testOnlyOneActiveRoomPerProjectAndOwnersAreIsolated() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let aliceRoom = try await makeRoom(store)
        do {
            _ = try await makeRoom(store)
            XCTFail("Second active room was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }
        let bobRoom = try await makeRoom(store, owner: "bob")
        XCTAssertNotEqual(aliceRoom.id, bobRoom.id)
        let loadedAlice = try await store.activeRoom(ownerUserID: "alice", projectID: "project-1")
        let loadedBob = try await store.activeRoom(ownerUserID: "bob", projectID: "project-1")
        XCTAssertEqual(loadedAlice, aliceRoom)
        XCTAssertEqual(loadedBob, bobRoom)
    }

    func testAgentRunCheckpointSurvivesReopenAndCannotChangeDeliveryIdentity() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "成员")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "成员")
        )
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "开始",
                mentionedAgentIDs: [agent.id]
            ),
            limits: .init()
        )
        let claimedValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: post.message.createdAtUnixMs + 1
        )
        let claimed = try XCTUnwrap(claimedValue)
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: "project-1",
            roomID: room.id,
            agentID: agent.id,
            deliveryID: claimed.id,
            triggerMessageID: post.message.id,
            rootMessageID: post.message.rootMessageID,
            runID: runID.uuidString.lowercased(),
            hopCount: claimed.hopCount
        )
        let scope = LocalAgentGroupChatRun.runtimeScope(for: context)
        var checkpoint = AgentRunCheckpoint(
            scope: scope,
            messages: [.init(role: .system, content: "system")]
        )
        checkpoint.id = runID
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

        let reopened = try SQLiteAgentGroupChatStore(databaseURL: url)
        let loaded = try await reopened.run(ownerUserID: "alice", deliveryID: claimed.id)
        XCTAssertEqual(loaded, run)

        let otherContext = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: "project-1",
            roomID: room.id,
            agentID: agent.id,
            deliveryID: claimed.id,
            triggerMessageID: post.message.id,
            rootMessageID: post.message.rootMessageID,
            runID: UUID().uuidString.lowercased(),
            hopCount: claimed.hopCount
        )
        var changedCheckpoint = checkpoint
        changedCheckpoint.id = UUID()
        let conflicting = try LocalAgentGroupChatRun(
            id: changedCheckpoint.id,
            context: otherContext,
            modelConfigID: agent.draft.modelConfigID,
            policy: .init(),
            checkpoint: changedCheckpoint,
            createdAtUnixMs: run.createdAtUnixMs,
            updatedAtUnixMs: run.updatedAtUnixMs + 1
        )
        do {
            try await reopened.saveRun(conflicting)
            XCTFail("Delivery accepted a different run identity")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }
    }

    func testStopOutstandingDeliveriesClosesRunsAndCancelsQueuedWorkAtomically() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let runningAgent = try await makeAgent(store, name: "运行成员")
        let queuedAgent = try await makeAgent(store, name: "排队成员")
        let room = try await makeRoom(store)
        for agent in [runningAgent, queuedAgent] {
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
                content: "同时停止",
                mentionedAgentIDs: [runningAgent.id, queuedAgent.id]
            ),
            limits: .init()
        )
        let runningValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: runningAgent.id,
            nowUnixMs: post.message.createdAtUnixMs + 1
        )
        let running = try XCTUnwrap(runningValue)
        let queued = try XCTUnwrap(post.deliveries.first(where: { $0.targetAgentID == queuedAgent.id }))
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: "project-1",
            roomID: room.id,
            agentID: runningAgent.id,
            deliveryID: running.id,
            triggerMessageID: post.message.id,
            rootMessageID: post.message.rootMessageID,
            runID: runID.uuidString.lowercased(),
            hopCount: running.hopCount
        )
        var checkpoint = AgentRunCheckpoint(
            scope: LocalAgentGroupChatRun.runtimeScope(for: context),
            messages: [.init(role: .system, content: "system")]
        )
        checkpoint.id = runID
        checkpoint.status = .paused
        let run = try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: runningAgent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: post.message.createdAtUnixMs + 1,
            updatedAtUnixMs: post.message.createdAtUnixMs + 1
        )
        try await store.saveRun(run)

        let stopped = try await store.stopOutstandingDeliveries(
            ownerUserID: "alice",
            roomID: room.id,
            reason: "用户停止全部 Agent。",
            nowUnixMs: post.message.createdAtUnixMs + 2
        )
        XCTAssertEqual(stopped, 2)
        let stoppedRunning = try await store.delivery(ownerUserID: "alice", deliveryID: running.id)
        let stoppedQueued = try await store.delivery(ownerUserID: "alice", deliveryID: queued.id)
        XCTAssertEqual(stoppedRunning?.status, .failed)
        XCTAssertEqual(stoppedQueued?.status, .cancelled)
        let closedRun = try await store.run(ownerUserID: "alice", deliveryID: running.id)
        XCTAssertEqual(closedRun?.checkpoint.status, .failed)
        XCTAssertEqual(closedRun?.checkpoint.stopReason, "用户停止全部 Agent。")
        XCTAssertEqual(closedRun?.events.last?.kind, "stopped_all")
        let unfinished = try await store.listUnfinishedRuns(
            ownerUserID: "alice",
            projectID: "project-1",
            limit: 10
        )
        XCTAssertTrue(unfinished.isEmpty)
    }
}
