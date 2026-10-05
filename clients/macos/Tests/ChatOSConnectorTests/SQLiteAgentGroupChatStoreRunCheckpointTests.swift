@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
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
        checkpoint.modelCalls = 3
        checkpoint.elapsedSeconds = 2.5
        checkpoint.completionResult = "done"
        let run = try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: agent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            events: [.init(kind: "needs_review", detail: "review detail", modelCalls: 3)],
            createdAtUnixMs: post.message.createdAtUnixMs + 1,
            updatedAtUnixMs: post.message.createdAtUnixMs + 1
        )
        try await store.saveRun(run)

        let reopened = try SQLiteAgentGroupChatStore(databaseURL: url)
        let loaded = try await reopened.run(ownerUserID: "alice", deliveryID: claimed.id)
        XCTAssertEqual(loaded, run)
        let loadedByID = try await reopened.run(ownerUserID: "alice", runID: run.id)
        XCTAssertEqual(loadedByID, run)
        let listedForAgent = try await reopened.listAgentRuns(
            ownerUserID: "alice",
            agentID: agent.id,
            limit: 10
        )
        XCTAssertEqual(listedForAgent, [run])
        let interruptedBefore = await reopened.preparedStatementCountForTesting()
        let interrupted = try await reopened.listInterruptedRuns(
            ownerUserID: "alice",
            limit: 500
        )
        let interruptedQueryCount = await reopened.preparedStatementCountForTesting()
            - interruptedBefore
        XCTAssertEqual(interrupted, [run])
        XCTAssertEqual(interruptedQueryCount, 1)
        let runningDeliveryRuns = try await reopened.listRunsWithRunningDeliveries(
            ownerUserID: "alice",
            projectID: "project-1",
            limit: 10
        )
        XCTAssertEqual(runningDeliveryRuns, [run])
        let listedForRoom = try await reopened.listRoomRuns(
            ownerUserID: "alice",
            roomID: room.id,
            limit: 10
        )
        XCTAssertEqual(listedForRoom, [run])
        let historySummaries = try await reopened.listRoomRunHistorySummaries(
            ownerUserID: "alice",
            roomID: room.id,
            limit: 10
        )
        XCTAssertEqual(historySummaries, [
            LocalAgentRunHistorySummary(
                id: run.id,
                agentID: agent.id,
                deliveryID: claimed.id,
                projectID: "project-1",
                roomID: room.id,
                triggerMessageID: post.message.id,
                lane: .manager,
                triggerKind: .mention,
                status: .ready,
                eventCount: 1,
                modelCalls: 3,
                memoryThreadID: nil,
                stopReason: nil,
                diagnosticReason: "review detail",
                elapsedSeconds: 2.5,
                hasResult: true,
                updatedAtUnixMs: run.updatedAtUnixMs
            ),
        ])
        let agentHistorySummaries = try await reopened.listAgentRunHistorySummaries(
            ownerUserID: "alice",
            agentID: agent.id,
            limit: 10
        )
        XCTAssertEqual(agentHistorySummaries, historySummaries)
        let deliveriesByID = try await reopened.deliveries(
            ownerUserID: "alice",
            deliveryIDs: [claimed.id, claimed.id]
        )
        XCTAssertEqual(deliveriesByID, [claimed.id: claimed])
        let messagesByID = try await reopened.messages(
            ownerUserID: "alice",
            messageIDs: [post.message.id, post.message.id]
        )
        XCTAssertEqual(messagesByID, [post.message.id: post.message])
        let otherAgent = try await makeAgent(reopened, name: "其他成员")
        let listedForOtherAgent = try await reopened.listAgentRuns(
            ownerUserID: "alice",
            agentID: otherAgent.id,
            limit: 10
        )
        XCTAssertTrue(listedForOtherAgent.isEmpty)
        _ = try await reopened.failDelivery(
            ownerUserID: "alice",
            deliveryID: claimed.id,
            error: "terminal delivery",
            nowUnixMs: run.updatedAtUnixMs + 1
        )
        let staleUnfinishedRuns = try await reopened.listUnfinishedRuns(
            ownerUserID: "alice",
            projectID: "project-1",
            limit: 10
        )
        XCTAssertEqual(staleUnfinishedRuns, [run])
        let terminalDeliveryRuns = try await reopened.listRunsWithRunningDeliveries(
            ownerUserID: "alice",
            projectID: "project-1",
            limit: 10
        )
        XCTAssertTrue(terminalDeliveryRuns.isEmpty)

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

}
