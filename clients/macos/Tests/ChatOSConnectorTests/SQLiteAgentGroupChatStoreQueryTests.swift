@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
    func testMessagePageHydratesRelationsWithConstantQueryCount() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "批量读取助手")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "成员")
        )
        for index in 0..<50 {
            _ = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: "alice",
                    content: "批量消息 \(index)",
                    mentionedAgentIDs: [agent.id]
                ),
                limits: .init()
            )
        }

        let before = await store.preparedStatementCountForTesting()
        let page = try await store.pageRecentMessages(
            ownerUserID: "alice",
            roomID: room.id,
            beforeMessageID: nil,
            limit: 50
        )
        let queryCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(page.messages.count, 50)
        XCTAssertTrue(page.messages.allSatisfy { $0.mentionedAgentIDs == [agent.id] })
        XCTAssertEqual(queryCount, 4)
    }

    func testActiveConversationListSnapshotUsesConstantQueryCount() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "列表助手")
        let firstRoom = try await makeRoom(store, projectID: "project-1")
        let secondRoom = try await makeRoom(store, projectID: "project-2")
        for room in [firstRoom, secondRoom] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "成员")
            )
        }
        _ = try await store.postMessage(
            ownerUserID: "alice",
            roomID: firstRoom.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "第一间旧消息",
                mentionedAgentIDs: [agent.id]
            ),
            limits: .init()
        )
        _ = try await store.postMessage(
            ownerUserID: "alice",
            roomID: firstRoom.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "第一间最新消息",
                mentionedAgentIDs: [agent.id]
            ),
            limits: .init()
        )
        _ = try await store.postMessage(
            ownerUserID: "alice",
            roomID: secondRoom.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "第二间最新消息"
            ),
            limits: .init()
        )
        let expectedFirstRoomPage = try await store.pageRecentMessages(
            ownerUserID: "alice",
            roomID: firstRoom.id,
            beforeMessageID: nil,
            limit: 1
        )
        let expectedFirstRoomLatest = try XCTUnwrap(expectedFirstRoomPage.messages.last)

        let before = await store.preparedStatementCountForTesting()
        let snapshot = try await store.activeConversationListSnapshot(ownerUserID: "alice")
        let queryCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(snapshot.activeMemberCountByRoomID[firstRoom.id], 1)
        XCTAssertEqual(snapshot.activeMemberCountByRoomID[secondRoom.id], 1)
        XCTAssertEqual(
            snapshot.latestMessageByRoomID[firstRoom.id]?.id,
            expectedFirstRoomLatest.id
        )
        XCTAssertEqual(
            snapshot.latestMessageByRoomID[firstRoom.id]?.mentionedAgentIDs,
            [agent.id]
        )
        XCTAssertEqual(snapshot.latestMessageByRoomID[secondRoom.id]?.content, "第二间最新消息")
        XCTAssertEqual(queryCount, 4)
    }

    func testActiveAccountMembershipSnapshotUsesOneQuery() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let firstAgent = try await makeAgent(store, name: "成员一")
        let secondAgent = try await makeAgent(store, name: "成员二")
        let firstRoom = try await makeRoom(store, projectID: "membership-project-1")
        let secondRoom = try await makeRoom(store, projectID: "membership-project-2")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: firstRoom.id,
            agentID: firstAgent.id,
            draft: .init(role: "开发")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: secondRoom.id,
            agentID: secondAgent.id,
            draft: .init(role: "测试")
        )

        let before = await store.preparedStatementCountForTesting()
        let members = try await store.listActiveMembers(ownerUserID: "alice")
        let queryCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(Set(members.map(\.roomID)), Set([firstRoom.id, secondRoom.id]))
        XCTAssertEqual(Set(members.map(\.agentID)), Set([firstAgent.id, secondAgent.id]))
        XCTAssertEqual(queryCount, 1)
    }

    func testMessageBatchHydratesRelationsWithConstantQueryCount() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "批量消息助手")
        let room = try await makeRoom(store, projectID: "message-batch-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "成员")
        )
        let first = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "第一条来源",
                mentionedAgentIDs: [agent.id]
            ),
            limits: .init()
        ).message
        let second = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "第二条来源"
            ),
            limits: .init()
        ).message

        let before = await store.preparedStatementCountForTesting()
        let messages = try await store.messages(
            ownerUserID: "alice",
            messageIDs: [second.id, first.id, first.id]
        )
        let queryCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(messages.count, 2)
        XCTAssertEqual(messages[first.id]?.mentionedAgentIDs, [agent.id])
        XCTAssertEqual(messages[second.id]?.content, "第二条来源")
        XCTAssertEqual(queryCount, 3)
    }

    func testVisibleTodoPresentationBatchesUseConstantQueryCount() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let firstAgent = try await makeAgent(store, name: "前置 Agent")
        let secondAgent = try await makeAgent(store, name: "当前 Agent")
        let room = try await makeRoom(store, projectID: "todo-batch-project")
        for agent in [firstAgent, secondAgent] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "成员")
            )
        }
        let source = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "批量 Todo 来源"
            ),
            limits: .init()
        ).message
        let prerequisite = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: firstAgent.id,
            requestKey: "todo-batch-prerequisite",
            draft: .init(
                title: "先完成接口",
                teamRoomID: room.id,
                sourceRoomID: room.id,
                sourceMessageID: source.id
            ),
            nowUnixMs: 100
        )
        let dependent = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: secondAgent.id,
            requestKey: "todo-batch-dependent",
            draft: .init(
                title: "再完成客户端",
                teamRoomID: room.id,
                dependencies: [.init(
                    prerequisiteTodoID: prerequisite.id,
                    prerequisiteAgentID: firstAgent.id
                )]
            ),
            nowUnixMs: 101
        )

        let before = await store.preparedStatementCountForTesting()
        let visible = try await store.listVisibleTeamTodos(
            ownerUserID: "alice",
            agentID: secondAgent.id,
            includeTerminal: true
        )
        let todoIDs = visible.map(\.id)
        let sources = try await store.listTodoSources(
            ownerUserID: "alice",
            todoIDs: todoIDs
        )
        let dependencies = try await store.listTodoDependencies(
            ownerUserID: "alice",
            todoIDs: todoIDs
        )
        let prerequisites = try await store.todos(
            ownerUserID: "alice",
            todoIDs: dependencies.map(\.prerequisiteTodoID)
        )
        let queryCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(Set(todoIDs), Set([prerequisite.id, dependent.id]))
        XCTAssertEqual(sources.map(\.messageID), [source.id])
        XCTAssertEqual(dependencies.map(\.prerequisiteTodoID), [prerequisite.id])
        XCTAssertEqual(prerequisites[prerequisite.id], prerequisite)
        XCTAssertEqual(queryCount, 4)
    }

    func testLinkTodoSourcesValidatesAllMessageRoomsWithOneQuery() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "来源批量校验 Agent")
        let room = try await makeRoom(store, projectID: "source-batch-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "source-batch-todo",
            draft: .init(title: "批量绑定来源", teamRoomID: room.id),
            nowUnixMs: 100
        )
        var sourceDrafts: [LocalAgentTodoSourceDraft] = []
        for index in 0..<12 {
            let message = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: "alice",
                    content: "来源消息 \(index)"
                ),
                limits: .init()
            ).message
            sourceDrafts.append(.init(roomID: room.id, messageID: message.id))
        }

        let countBefore = await store.preparedStatementCountForTesting()
        let links = try await store.linkAgentTodoSources(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            sources: sourceDrafts,
            nowUnixMs: 101
        )
        let queryCount = await store.preparedStatementCountForTesting() - countBefore

        // BEGIN + Todo + one message-room batch + one source batch + result list + COMMIT.
        XCTAssertEqual(queryCount, 6)
        XCTAssertEqual(Set(links.map(\.messageID)), Set(sourceDrafts.map(\.messageID)))
    }

    func testSetTodoDependenciesValidatesAndChecksCyclesInTwoBatchQueries() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "依赖批量校验 Agent")
        let room = try await makeRoom(store, projectID: "dependency-batch-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        var dependencyDrafts: [LocalAgentTodoDependencyDraft] = []
        for index in 0..<12 {
            let prerequisite = try await store.createAgentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                requestKey: "dependency-batch-prerequisite-\(index)",
                draft: .init(title: "前置任务 \(index)", teamRoomID: room.id),
                nowUnixMs: Int64(100 + index)
            )
            dependencyDrafts.append(.init(
                prerequisiteTodoID: prerequisite.id,
                prerequisiteAgentID: agent.id
            ))
        }
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "dependency-batch-target",
            draft: .init(title: "目标任务", teamRoomID: room.id),
            nowUnixMs: 200
        )

        let countBefore = await store.preparedStatementCountForTesting()
        let dependencies = try await store.setAgentTodoDependencies(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id,
            dependencies: dependencyDrafts,
            nowUnixMs: 201
        )
        let queryCount = await store.preparedStatementCountForTesting() - countBefore

        // BEGIN + Todo + DELETE + prerequisite batch + cycle batch + one dependency batch
        // + result list + COMMIT.
        XCTAssertEqual(queryCount, 8)
        XCTAssertEqual(
            Set(dependencies.map(\.prerequisiteTodoID)),
            Set(dependencyDrafts.map(\.prerequisiteTodoID))
        )
    }

    func testReadAllUnreadBatchesConversationMetadata() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "全局未读 Agent")
        var expectedRoomIDs = Set<String>()
        for index in 0..<12 {
            let room = try await makeRoom(
                store,
                projectID: "global-unread-batch-project-\(index)"
            )
            expectedRoomIDs.insert(room.id)
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "成员")
            )
            _ = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: "alice",
                    content: "全局未读消息 \(index)"
                ),
                limits: .init()
            )
        }

        let countBefore = await store.preparedStatementCountForTesting()
        let unread = try await store.readAllUnreadMessagesAndMarkRead(
            ownerUserID: "alice",
            agentID: agent.id,
            limit: 200,
            nowUnixMs: 1_000
        )
        let queryCount = await store.preparedStatementCountForTesting() - countBefore

        // BEGIN + Agent + messages + two relation batches + one cursor batch + rooms + COMMIT.
        XCTAssertEqual(queryCount, 8)
        XCTAssertEqual(Set(unread.map(\.room.id)), expectedRoomIDs)
        XCTAssertEqual(unread.reduce(0) { $0 + $1.messages.count }, 12)
    }

}
