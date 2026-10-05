@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
    func testAgentHeartbeatCreatesOneGlobalInboxWake() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "巡检 Agent",
                rolePrompt: "主动检查所有会话。",
                modelConfigID: "model-1",
                heartbeatEnabled: true,
                heartbeatIntervalSeconds: 60,
                heartbeatPrompt: "检查阻塞和无人响应的工作。"
            )
        )
        let peer = try await makeAgent(store, name: "协作者")
        let firstTeam = try await makeRoom(store, projectID: "heartbeat-project-1")
        let secondTeam = try await makeRoom(store, projectID: "heartbeat-project-2")
        for room in [firstTeam, secondTeam] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "巡检成员")
            )
        }
        let humanDirect = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        _ = try await store.openAgentDirect(
            ownerUserID: "alice",
            initiatingAgentID: agent.id,
            targetAgentID: peer.id
        )
        let dueAt = try XCTUnwrap(agent.nextHeartbeatAtUnixMs)

        let deliveries = try await store.enqueueDueAgentHeartbeats(
            ownerUserID: "alice",
            nowUnixMs: dueAt
        )

        XCTAssertEqual(deliveries.count, 1)
        XCTAssertEqual(deliveries.first?.roomID, humanDirect.id)
        XCTAssertTrue(deliveries.allSatisfy {
            $0.targetAgentID == agent.id && $0.triggerKind == .heartbeat
        })
        let visible = try await store.listMessages(
            ownerUserID: "alice",
            roomID: humanDirect.id,
            limit: 20
        )
        XCTAssertTrue(visible.isEmpty, "heartbeat triggers must stay out of the transcript")
        let duplicate = try await store.enqueueDueAgentHeartbeats(
            ownerUserID: "alice",
            nowUnixMs: dueAt
        )
        XCTAssertTrue(duplicate.isEmpty)
        let profiles = try await store.listAgents(ownerUserID: "alice", includeArchived: false)
        let updated = try XCTUnwrap(profiles.first(where: { $0.id == agent.id }))
        XCTAssertEqual(updated.lastHeartbeatAtUnixMs, dueAt)
        XCTAssertEqual(updated.nextHeartbeatAtUnixMs, dueAt + 60_000)
        XCTAssertEqual(updated.draft.heartbeatPrompt, "检查阻塞和无人响应的工作。")

        let first = try XCTUnwrap(deliveries.first)
        let claimed = try await store.claimNextDelivery(
            ownerUserID: "alice",
            roomID: first.roomID,
            agentID: agent.id,
            nowUnixMs: dueAt + 1
        )
        XCTAssertEqual(claimed?.id, first.id)
        let completed = try await store.completeHeartbeatDelivery(
            ownerUserID: "alice",
            deliveryID: first.id,
            nowUnixMs: dueAt + 2
        )
        XCTAssertEqual(completed.status, .completed)
        XCTAssertNil(completed.responseMessageID)
    }

    func testHeartbeatBatchUsesOneCandidateQueryAndNoDeliveryReadback() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        var agents: [LocalAgentProfile] = []
        var dueAt: Int64 = 0
        for index in 0..<12 {
            let agent = try await store.createAgent(
                ownerUserID: "alice",
                draft: .init(
                    name: "批量心跳 Agent \(index)",
                    rolePrompt: "批量验证心跳。",
                    modelConfigID: "model-1",
                    heartbeatEnabled: true,
                    heartbeatIntervalSeconds: 60,
                    heartbeatPrompt: "巡检 \(index)"
                )
            )
            agents.append(agent)
            dueAt = max(dueAt, try XCTUnwrap(agent.nextHeartbeatAtUnixMs))
            _ = try await store.openHumanAgentDirect(
                ownerUserID: "alice",
                agentID: agent.id
            )
        }

        let countBefore = await store.preparedStatementCountForTesting()
        let deliveries = try await store.enqueueDueAgentHeartbeats(
            ownerUserID: "alice",
            nowUnixMs: dueAt,
            agentLimit: 16
        )
        let queryCount = await store.preparedStatementCountForTesting() - countBefore

        // BEGIN + one candidate query + three batch writes + COMMIT.
        XCTAssertEqual(queryCount, 6)
        XCTAssertEqual(Set(deliveries.map(\.targetAgentID)), Set(agents.map(\.id)))
        XCTAssertTrue(deliveries.allSatisfy { $0.status == .pending && $0.triggerKind == .heartbeat })
    }

    func testHeartbeatBatchCreatesMissingDirectRoomsWithTwoBatchWrites() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        var agents: [LocalAgentProfile] = []
        var dueAt: Int64 = 0
        for index in 0..<12 {
            let agent = try await store.createAgent(
                ownerUserID: "alice",
                draft: .init(
                    name: "首次心跳 Agent \(index)",
                    rolePrompt: "首次心跳创建直属会话。",
                    modelConfigID: "model-1",
                    heartbeatEnabled: true,
                    heartbeatIntervalSeconds: 60,
                    heartbeatPrompt: "首次巡检 \(index)"
                )
            )
            agents.append(agent)
            dueAt = max(dueAt, try XCTUnwrap(agent.nextHeartbeatAtUnixMs))
        }

        let before = await store.preparedStatementCountForTesting()
        let deliveries = try await store.enqueueDueAgentHeartbeats(
            ownerUserID: "alice",
            nowUnixMs: dueAt,
            agentLimit: 16
        )
        let statementCount = await store.preparedStatementCountForTesting() - before

        // BEGIN + candidates + rooms + members + messages + deliveries + schedules + COMMIT.
        XCTAssertEqual(statementCount, 8)
        XCTAssertEqual(Set(deliveries.map(\.targetAgentID)), Set(agents.map(\.id)))
        let rooms = try await store.listDirectConversations(ownerUserID: "alice")
        XCTAssertEqual(rooms.count, 12)
        XCTAssertEqual(Set(rooms.compactMap(\.defaultAgentID)), Set(agents.map(\.id)))
    }

    func testPendingTodoBatchUsesOneCandidateQueryAndNoDeliveryReadback() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let room = try await makeRoom(store, projectID: "todo-batch-project")
        var todoIDs = Set<String>()
        for index in 0..<12 {
            let agent = try await makeAgent(store, name: "批量 Todo Agent \(index)")
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "执行者")
            )
            let todo = try await store.createAgentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                requestKey: "todo-batch-\(index)",
                draft: .init(title: "批量任务 \(index)", teamRoomID: room.id),
                nowUnixMs: Int64(100 + index)
            )
            todoIDs.insert(todo.id)
        }

        let before = await store.preparedStatementCountForTesting()
        let deliveries = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: 1_000,
            agentLimit: 12
        )
        let statementCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(deliveries.count, 12)
        XCTAssertEqual(Set(deliveries.map(\.targetAgentID)).count, 12)
        XCTAssertEqual(Set(deliveries.map { String($0.deduplicationKey.dropFirst("todo:".count)) }), todoIDs)
        // BEGIN + one candidate query + one Delivery read + three batch writes + COMMIT.
        XCTAssertEqual(statementCount, 7)
    }

    func testPendingTodoBatchReactivatesFailedDeliveryWithoutDuplicateRows() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "批量恢复 Agent")
        let room = try await makeRoom(store, projectID: "todo-batch-retry-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "todo-batch-retry",
            draft: .init(title: "恢复批量任务", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let firstBatch = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: 101
        )
        let first = try XCTUnwrap(firstBatch.first)
        let claimedValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 102
        )
        let claimed = try XCTUnwrap(claimedValue)
        _ = try await store.failDelivery(
            ownerUserID: "alice",
            deliveryID: claimed.id,
            error: "临时失败",
            nowUnixMs: 103
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            update: .init(status: .pending),
            nowUnixMs: 104
        )

        let before = await store.preparedStatementCountForTesting()
        let retryBatch = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: 105
        )
        let retried = try XCTUnwrap(retryBatch.first)
        let statementCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(retried.id, first.id)
        XCTAssertEqual(retried.messageID, first.messageID)
        XCTAssertEqual(retried.attempt, 1)
        XCTAssertEqual(retried.status, .pending)
        XCTAssertEqual(statementCount, 7)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM project_agent_deliveries WHERE deduplication_key = 'todo:\(todo.id)'"
        ), 1)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM project_agent_messages WHERE id = '\(first.messageID)'"
        ), 1)
    }

    func testCreateTodoValidatesAdditionalSourcesWithTwoBatchQueries() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "多来源 Todo Agent")
        let room = try await makeRoom(store, projectID: "multi-source-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        var sources: [LocalAgentTodoSourceDraft] = []
        for index in 0..<12 {
            let post = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: "alice",
                    content: "来源消息 \(index)",
                    mentionedAgentIDs: [agent.id]
                ),
                limits: .init(maximumAgentRunsPerRootMessage: 20)
            )
            sources.append(.init(roomID: room.id, messageID: post.message.id))
        }

        let before = await store.preparedStatementCountForTesting()
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "multi-source-batch",
            draft: .init(
                title: "批量校验来源",
                teamRoomID: room.id,
                additionalSources: sources
            ),
            nowUnixMs: 1_000
        )
        let statementCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(statementCount, 15)
        let links = try await store.listAgentTodoSources(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(links.count, 12)
    }

    func testAgentHeartbeatIsDisabledByDefault() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "普通 Agent")
        let room = try await makeRoom(store, projectID: "heartbeat-disabled")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "成员")
        )

        XCTAssertFalse(agent.draft.heartbeatEnabled)
        XCTAssertNil(agent.nextHeartbeatAtUnixMs)
        let nextDue = try await store.nextAgentHeartbeatDue(ownerUserID: "alice")
        XCTAssertNil(nextDue)
        let deliveries = try await store.enqueueDueAgentHeartbeats(
            ownerUserID: "alice",
            nowUnixMs: Int64.max / 2
        )
        XCTAssertTrue(deliveries.isEmpty)
    }

}
