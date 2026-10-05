@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
    func testAgentTodoPriorityCanChangeAfterNewMessagesAndExecutesInSourceProject() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "执行者")
        let room = try await makeRoom(store, projectID: "todo-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let firstMessage = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "先整理文档"),
            limits: .init()
        ).message
        let secondMessage = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "线上故障"),
            limits: .init()
        ).message
        let first = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "todo-first",
            draft: .init(
                title: "整理文档",
                priority: 30,
                sourceRoomID: room.id,
                sourceMessageID: firstMessage.id
            ),
            nowUnixMs: firstMessage.createdAtUnixMs + 1
        )
        let urgent = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "todo-urgent",
            draft: .init(
                title: "处理线上故障",
                priority: 90,
                sourceRoomID: room.id,
                sourceMessageID: secondMessage.id
            ),
            nowUnixMs: secondMessage.createdAtUnixMs + 1
        )
        var todos = try await store.listAgentTodos(
            ownerUserID: "alice",
            agentID: agent.id,
            includeTerminal: false
        )
        XCTAssertEqual(todos.map(\.id), [urgent.id, first.id])

        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: first.id,
            update: .init(priority: 100),
            nowUnixMs: secondMessage.createdAtUnixMs + 2
        )
        todos = try await store.listAgentTodos(
            ownerUserID: "alice",
            agentID: agent.id,
            includeTerminal: false
        )
        XCTAssertEqual(todos.map(\.id), [first.id, urgent.id])

        let reordered = try await store.reorderAgentTodos(
            ownerUserID: "alice",
            agentID: agent.id,
            todoIDs: [urgent.id, first.id],
            nowUnixMs: secondMessage.createdAtUnixMs + 3
        )
        XCTAssertEqual(reordered.map(\.id), [urgent.id, first.id])

        // Existing message deliveries must be cleared before autonomous Todo execution starts.
        while let delivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: secondMessage.createdAtUnixMs + 4
        ) {
            _ = try await store.failDelivery(
                ownerUserID: "alice",
                deliveryID: delivery.id,
                error: "test clears message delivery",
                nowUnixMs: secondMessage.createdAtUnixMs + 5
            )
        }
        let deliveries = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: secondMessage.createdAtUnixMs + 6
        )
        XCTAssertEqual(deliveries.count, 1)
        XCTAssertEqual(deliveries.first?.triggerKind, .todo)
        XCTAssertEqual(deliveries.first?.roomID, room.id)
        XCTAssertTrue(deliveries.first?.deduplicationKey.hasSuffix(urgent.id) == true)
    }

    func testTodoReorderingUsesOneBatchUpdatePerScope() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "批量重排 Agent")
        let room = try await makeRoom(store, projectID: "todo-reorder-batch-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        var todoIDs: [String] = []
        for index in 0..<12 {
            let todo = try await store.createAgentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                requestKey: "todo-reorder-batch-\(index)",
                draft: .init(title: "批量重排任务 \(index)", teamRoomID: room.id),
                nowUnixMs: Int64(100 + index)
            )
            todoIDs.append(todo.id)
        }

        let reversed = Array(todoIDs.reversed())
        let agentBefore = await store.preparedStatementCountForTesting()
        let agentTodos = try await store.reorderAgentTodos(
            ownerUserID: "alice",
            agentID: agent.id,
            todoIDs: reversed,
            nowUnixMs: 1_000
        )
        let agentStatementCount = await store.preparedStatementCountForTesting() - agentBefore

        XCTAssertEqual(agentTodos.map(\.id), reversed)
        XCTAssertEqual(agentStatementCount, 5)

        let teamBefore = await store.preparedStatementCountForTesting()
        let teamTodos = try await store.reorderTeamTodos(
            ownerUserID: "alice",
            teamRoomID: room.id,
            todoIDs: todoIDs,
            nowUnixMs: 1_001
        )
        let teamStatementCount = await store.preparedStatementCountForTesting() - teamBefore

        XCTAssertEqual(teamTodos.map(\.id), todoIDs)
        XCTAssertEqual(teamStatementCount, 7)
    }

    func testTodoListsSortActiveWorkBeforeTerminalHistory() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "执行者")
        let room = try await makeRoom(store, projectID: "todo-order-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )

        func create(_ key: String, priority: Int, now: Int64) async throws -> LocalAgentTodo {
            try await store.createAgentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                requestKey: key,
                draft: .init(title: key, priority: priority, teamRoomID: room.id),
                nowUnixMs: now
            )
        }

        let completed = try await create("completed", priority: 100, now: 100)
        let pending = try await create("pending", priority: 50, now: 101)
        let cancelled = try await create("cancelled", priority: 100, now: 102)
        let blocked = try await create("blocked", priority: 90, now: 103)
        let running = try await create("running", priority: 10, now: 104)

        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: completed.id,
            update: .init(status: .inProgress),
            nowUnixMs: 110
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: completed.id,
            update: .init(status: .completed),
            nowUnixMs: 111
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: cancelled.id,
            update: .init(status: .cancelled),
            nowUnixMs: 112
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: blocked.id,
            update: .init(status: .inProgress),
            nowUnixMs: 113
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: blocked.id,
            update: .init(status: .blocked, blockedReason: "等待输入"),
            nowUnixMs: 114
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: running.id,
            update: .init(status: .inProgress),
            nowUnixMs: 115
        )

        let expected = [running.id, pending.id, blocked.id, completed.id, cancelled.id]
        let teamTodos = try await store.listTeamTodos(
            ownerUserID: "alice",
            teamRoomID: room.id,
            includeTerminal: true
        )
        XCTAssertEqual(teamTodos.map(\.id), expected)
        let agentTodos = try await store.listAgentTodos(
            ownerUserID: "alice",
            agentID: agent.id,
            includeTerminal: true
        )
        XCTAssertEqual(agentTodos.map(\.id), expected)
    }

    func testFailedTodoDeliveryIsReactivatedWithoutDuplicateRows() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "执行者")
        let room = try await makeRoom(store, projectID: "todo-retry-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "retry-failed-delivery",
            draft: .init(title: "处理超时任务", teamRoomID: room.id),
            nowUnixMs: 100
        )

        let firstCountBefore = await store.preparedStatementCountForTesting()
        let firstPendingValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        let firstStatementCount = await store.preparedStatementCountForTesting()
            - firstCountBefore
        let firstPending = try XCTUnwrap(firstPendingValue)
        let firstClaimValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 102
        )
        let firstClaim = try XCTUnwrap(firstClaimValue)
        XCTAssertEqual(firstClaim.id, firstPending.id)
        XCTAssertEqual(firstClaim.attempt, 1)
        _ = try await store.failDelivery(
            ownerUserID: "alice",
            deliveryID: firstClaim.id,
            error: "模型单次请求超时",
            nowUnixMs: 103
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            update: .init(status: .pending),
            nowUnixMs: 104
        )

        let retryCountBefore = await store.preparedStatementCountForTesting()
        let retriedPendingValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 105
        )
        let retryStatementCount = await store.preparedStatementCountForTesting()
            - retryCountBefore
        let retriedPending = try XCTUnwrap(retriedPendingValue)
        XCTAssertEqual(retriedPending.id, firstPending.id)
        XCTAssertEqual(retriedPending.status, .pending)
        XCTAssertEqual(retriedPending.attempt, 1)
        XCTAssertNil(retriedPending.lastError)
        XCTAssertNil(retriedPending.claimedAtUnixMs)
        XCTAssertNil(retriedPending.completedAtUnixMs)
        XCTAssertEqual(firstStatementCount, 8)
        XCTAssertEqual(retryStatementCount, 8)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM project_agent_deliveries WHERE deduplication_key = 'todo:\(todo.id)'"
        ), 1)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM project_agent_messages WHERE id = '\(firstPending.messageID)'"
        ), 1)

        let secondClaimValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 106
        )
        let secondClaim = try XCTUnwrap(secondClaimValue)
        XCTAssertEqual(secondClaim.id, firstClaim.id)
        XCTAssertEqual(secondClaim.attempt, 2)
        XCTAssertEqual(secondClaim.status, .running)
        let retriedTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(retriedTodo?.status, .inProgress)
    }

    func testBlockedCompletedTodoReopenCreatesIndependentExecutorAttempt() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "重新执行者")
        let room = try await makeRoom(store, projectID: "todo-reopen-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "reopen-completed-delivery",
            draft: .init(title: "解除阻塞后重新执行", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let firstValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        let first = try XCTUnwrap(firstValue)
        let firstClaimValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 102
        )
        _ = try XCTUnwrap(firstClaimValue)
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            update: .init(status: .blocked, blockedReason: "等待本地门禁解除"),
            nowUnixMs: 103
        )
        _ = try await store.completeHeartbeatDelivery(
            ownerUserID: "alice",
            deliveryID: first.id,
            nowUnixMs: 104
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            update: .init(status: .pending, blockedReason: ""),
            nowUnixMs: 105
        )

        let secondValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 106
        )
        let second = try XCTUnwrap(secondValue)
        XCTAssertNotEqual(second.id, first.id)
        XCTAssertEqual(first.todoID, todo.id)
        XCTAssertEqual(second.todoID, todo.id)
        XCTAssertEqual(second.deduplicationKey, "todo:\(todo.id):attempt:106")
        let completedFirst = try await store.delivery(ownerUserID: "alice", deliveryID: first.id)
        XCTAssertEqual(completedFirst?.status, .completed)
        let reopenedTodo = try await store.todoForDelivery(ownerUserID: "alice", deliveryID: second.id)
        XCTAssertEqual(reopenedTodo?.id, todo.id)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM project_agent_deliveries WHERE trigger_kind = 'todo'"
        ), 2)
    }

    func testNeedsReviewManagerRunDoesNotBlockLaterCommunication() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "通讯恢复者")
        let room = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)
        let firstPost = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "第一条消息")
        )
        let firstValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            lane: .manager,
            nowUnixMs: 101
        )
        let first = try XCTUnwrap(firstValue)
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: room.projectID,
            roomID: room.id,
            agentID: agent.id,
            deliveryID: first.id,
            triggerMessageID: firstPost.message.id,
            rootMessageID: firstPost.message.rootMessageID,
            runID: runID.uuidString.lowercased(),
            hopCount: first.hopCount,
            lane: .manager
        )
        var checkpoint = AgentRunCheckpoint(
            scope: LocalAgentGroupChatRun.runtimeScope(for: context),
            messages: [.init(role: .system, content: "communication")]
        )
        checkpoint.id = runID
        checkpoint.status = .needsReview
        checkpoint.stopReason = "本地确定性写入被拒绝"
        try await store.saveRun(try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: agent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: 101,
            updatedAtUnixMs: 102
        ))
        let secondPost = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "第二条消息")
        )

        let quarantinedCount = try await store.quarantineNeedsReviewManagerDeliveries(
            ownerUserID: "alice",
            nowUnixMs: 103
        )
        XCTAssertEqual(quarantinedCount, 1)
        let quarantinedFirst = try await store.delivery(ownerUserID: "alice", deliveryID: first.id)
        XCTAssertEqual(quarantinedFirst?.status, .failed)
        let secondValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            lane: .manager,
            nowUnixMs: 104
        )
        let second = try XCTUnwrap(secondValue)
        XCTAssertEqual(second.id, secondPost.deliveries.first?.id)
    }

}
