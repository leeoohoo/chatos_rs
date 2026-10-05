@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
    func testScheduleStateNeverAdvertisesTodoThatExecutorLaneCannotStart() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "执行者")
        let room = try await makeRoom(store, projectID: "todo-schedule-consistency")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "outstanding-pending-todo",
            draft: .init(title: "已有执行占位的任务", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let readyStateCountBefore = await store.preparedStatementCountForTesting()
        let readyState = try await store.agentTodoScheduleState(
            ownerUserID: "alice",
            agentID: agent.id
        )
        let readyStateStatementCount = await store.preparedStatementCountForTesting()
            - readyStateCountBefore
        XCTAssertNil(readyState.runningTodo)
        XCTAssertEqual(readyState.readyTodo?.id, todo.id)
        XCTAssertEqual(readyStateStatementCount, 4)

        let pendingDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        _ = try XCTUnwrap(pendingDelivery)
        let claimedDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 102
        )
        _ = try XCTUnwrap(claimedDelivery)
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            update: .init(status: .blocked, blockedReason: "等待处理"),
            nowUnixMs: 103
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            update: .init(status: .pending, blockedReason: ""),
            nowUnixMs: 104
        )

        let state = try await store.agentTodoScheduleState(
            ownerUserID: "alice",
            agentID: agent.id
        )
        XCTAssertNil(state.runningTodo)
        XCTAssertNil(state.readyTodo)
        let started = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 105
        )
        XCTAssertNil(started)
    }

    func testTodoEventRecipientsPersistOncePerEventAndRecipient() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护任务板。",
                modelConfigID: "model-1",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(store, name: "执行者")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "event-recipient-project",
            draft: .init(name: "事件投递团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: worker.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "event-recipient-todo",
            draft: .init(
                title: "实现功能",
                teamRoomID: room.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 100
        )

        let firstReady = try await store.enqueueAgentTodoReady(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            nowUnixMs: 101
        )
        let repeatedReady = try await store.enqueueAgentTodoReady(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            nowUnixMs: 102
        )
        XCTAssertEqual(firstReady?.id, repeatedReady?.id)

        _ = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            nowUnixMs: 103
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            update: .init(status: .blocked, blockedReason: "等待接口"),
            nowUnixMs: 104
        )
        let firstStatus = try await store.enqueueAgentTodoStatus(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            excludingAgentID: nil,
            nowUnixMs: 105
        )
        let repeatedStatus = try await store.enqueueAgentTodoStatus(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            excludingAgentID: nil,
            nowUnixMs: 106
        )
        XCTAssertEqual(Set(firstStatus.map(\.id)), Set(repeatedStatus.map(\.id)))
        XCTAssertEqual(firstStatus.count, 2)
        let managerStatus = try XCTUnwrap(
            firstStatus.first(where: { $0.targetAgentID == manager.id })
        )
        let loadedManagerMessage = try await store.message(
            ownerUserID: "alice",
            roomID: managerStatus.roomID,
            messageID: managerStatus.messageID
        )
        let managerMessage = try XCTUnwrap(loadedManagerMessage)
        XCTAssertTrue(managerMessage.content.contains("不得转给 Human"))
        XCTAssertTrue(managerMessage.content.contains("项目经理待协调事项"))
        XCTAssertTrue(managerMessage.content.contains("2—3 个方案"))

        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            update: .init(status: .cancelled),
            nowUnixMs: 107
        )
        let cancellation = try await store.enqueueAgentTodoStatus(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            excludingAgentID: manager.id,
            nowUnixMs: 108
        )
        XCTAssertEqual(cancellation.map(\.targetAgentID), [worker.id])

        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_todo_event_recipients"
        ), 4)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_todo_event_recipients WHERE event_kind = 'ready'"
        ), 1)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_todo_event_recipients WHERE event_kind = 'blocked'"
        ), 2)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_todo_event_recipients WHERE event_kind = 'cancelled'"
        ), 1)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(DISTINCT delivery_id) FROM local_agent_todo_event_recipients"
        ), 4)
    }

    func testTodoStatusUsesDirectRoomFastPathAndBatchesDeduplicationReads() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "批量通知经理",
                rolePrompt: "维护任务板。",
                modelConfigID: "model-1",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(store, name: "批量通知执行者")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "status-batch-project",
            draft: .init(name: "批量通知团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: worker.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "status-batch-todo",
            draft: .init(
                title: "发送批量状态通知",
                teamRoomID: room.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 100
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            update: .init(status: .cancelled),
            nowUnixMs: 101
        )
        _ = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: worker.id)
        _ = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: manager.id)

        let before = await store.preparedStatementCountForTesting()
        let deliveries = try await store.enqueueAgentTodoStatus(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            excludingAgentID: nil,
            nowUnixMs: 102
        )
        let statementCount = await store.preparedStatementCountForTesting() - before

        let repeatedBefore = await store.preparedStatementCountForTesting()
        let repeated = try await store.enqueueAgentTodoStatus(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            excludingAgentID: nil,
            nowUnixMs: 103
        )
        let repeatedStatementCount = await store.preparedStatementCountForTesting()
            - repeatedBefore

        XCTAssertEqual(deliveries.count, 2)
        XCTAssertEqual(Set(deliveries.map(\.targetAgentID)), Set([worker.id, manager.id]))
        XCTAssertEqual(Set(deliveries.map(\.id)), Set(repeated.map(\.id)))
        XCTAssertEqual(statementCount, 11)
        XCTAssertEqual(repeatedStatementCount, 9)
    }

    func testTodoReadyUsesOneReadinessQueryAndNoDeliveryReadback() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "Ready 通知 Agent")
        let room = try await makeRoom(store, projectID: "ready-notification-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "ready-notification-todo",
            draft: .init(title: "发送 Ready 通知", teamRoomID: room.id),
            nowUnixMs: 100
        )
        _ = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)

        let before = await store.preparedStatementCountForTesting()
        let first = try await store.enqueueAgentTodoReady(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            nowUnixMs: 101
        )
        let firstStatementCount = await store.preparedStatementCountForTesting() - before

        let repeatedBefore = await store.preparedStatementCountForTesting()
        let repeated = try await store.enqueueAgentTodoReady(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            nowUnixMs: 102
        )
        let repeatedStatementCount = await store.preparedStatementCountForTesting()
            - repeatedBefore

        XCTAssertEqual(first?.id, repeated?.id)
        XCTAssertEqual(firstStatementCount, 9)
        XCTAssertEqual(repeatedStatementCount, 7)
    }

    func testReadyDependentTodoNotificationsBatchReadsAndWritesAtomically() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let prerequisiteOwner = try await makeAgent(store, name: "批量前置负责人")
        let room = try await makeRoom(store, projectID: "ready-dependent-batch-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: prerequisiteOwner.id,
            draft: .init(role: "前置负责人")
        )
        let prerequisite = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: prerequisiteOwner.id,
            requestKey: "ready-dependent-prerequisite",
            draft: .init(title: "完成共享前置", teamRoomID: room.id),
            nowUnixMs: 100
        )
        var dependentAgentIDs = Set<String>()
        for index in 0..<12 {
            let agent = try await makeAgent(store, name: "批量后置 Agent \(index)")
            dependentAgentIDs.insert(agent.id)
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "后置执行者")
            )
            _ = try await store.createAgentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                requestKey: "ready-dependent-\(index)",
                draft: .init(
                    title: "执行后置任务 \(index)",
                    teamRoomID: room.id,
                    dependencies: [.init(
                        prerequisiteTodoID: prerequisite.id,
                        prerequisiteAgentID: prerequisiteOwner.id
                    )]
                ),
                nowUnixMs: Int64(101 + index)
            )
        }
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: prerequisiteOwner.id,
            todoID: prerequisite.id,
            update: .init(status: .inProgress),
            nowUnixMs: 200
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: prerequisiteOwner.id,
            todoID: prerequisite.id,
            update: .init(status: .completed, result: "共享前置已完成"),
            nowUnixMs: 201
        )

        let before = await store.preparedStatementCountForTesting()
        let first = try await store.enqueueReadyDependentAgentTodos(
            ownerUserID: "alice",
            prerequisiteTodoID: prerequisite.id,
            nowUnixMs: 202
        )
        let firstStatementCount = await store.preparedStatementCountForTesting() - before

        let repeatedBefore = await store.preparedStatementCountForTesting()
        let repeated = try await store.enqueueReadyDependentAgentTodos(
            ownerUserID: "alice",
            prerequisiteTodoID: prerequisite.id,
            nowUnixMs: 203
        )
        let repeatedStatementCount = await store.preparedStatementCountForTesting()
            - repeatedBefore

        XCTAssertEqual(first.count, 12)
        XCTAssertEqual(Set(first.map(\.targetAgentID)), dependentAgentIDs)
        XCTAssertEqual(Set(first.map(\.id)), Set(repeated.map(\.id)))
        // The first call creates all twelve missing direct rooms and members in two batch writes;
        // the previous per-Agent open path required about 93 statements for the whole operation.
        XCTAssertEqual(firstStatementCount, 15)
        XCTAssertEqual(repeatedStatementCount, 7)
    }

}
