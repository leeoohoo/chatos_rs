@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

final class SQLiteAgentGroupChatStoreTests: XCTestCase {
    private struct NoNetworkTicketProvider: LocalConnectorPairingTicketProviding {
        func issueLocalConnectorPairingTicket() async throws -> String {
            throw URLError(.notConnectedToInternet)
        }
    }

    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-group-chat-\(UUID().uuidString)")
            .appendingPathComponent("group-chat.db")
    }

    private func toolArguments(_ value: [String: Any]) throws -> String {
        let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
        return try XCTUnwrap(String(data: data, encoding: .utf8))
    }

    private func executeSQLite(_ databaseURL: URL, sql: String) throws {
        var database: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &database,
            SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK, let database else {
            defer { sqlite3_close(database) }
            throw AgentGroupChatError.storage("test database open failed")
        }
        defer { sqlite3_close(database) }
        guard sqlite3_exec(database, sql, nil, nil, nil) == SQLITE_OK else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
    }

    private func sqliteInt(_ databaseURL: URL, sql: String) throws -> Int64 {
        var database: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &database,
            SQLITE_OPEN_READONLY | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK, let database else {
            defer { sqlite3_close(database) }
            throw AgentGroupChatError.storage("test database open failed")
        }
        defer { sqlite3_close(database) }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
        return sqlite3_column_int64(statement, 0)
    }

    private func sqliteText(_ databaseURL: URL, sql: String) throws -> String {
        var database: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &database,
            SQLITE_OPEN_READONLY | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK, let database else {
            defer { sqlite3_close(database) }
            throw AgentGroupChatError.storage("test database open failed")
        }
        defer { sqlite3_close(database) }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW,
              let value = sqlite3_column_text(statement, 0) else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
        return String(cString: value)
    }

    func testRetiredRequirementSurveyCapabilitiesRemainReadableWithoutMutation() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "兼容旧任务 Agent")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "legacy-requirement-survey-capabilities",
            draft: .init(title: "读取旧调研任务", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let legacyPlan = """
        {"builtinCapabilities":["requirement_survey_read","requirement_survey_write"],"plugins":[],"requiresExecution":true,"selectedAtUnixMs":99,"selectionRevision":"local-capability-catalog-v1"}
        """
        try executeSQLite(
            url,
            sql: "UPDATE local_agent_todos SET execution_plan_json = '\(legacyPlan)' WHERE id = '\(todo.id)'"
        )

        let loaded = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )

        XCTAssertEqual(loaded?.executionPlan.builtinCapabilities, [])
        XCTAssertEqual(loaded?.executionPlan.selectionRevision, "local-capability-catalog-v1")
        XCTAssertEqual(try sqliteText(
            url,
            sql: "SELECT execution_plan_json FROM local_agent_todos WHERE id = '\(todo.id)'"
        ), legacyPlan)
    }

    func testUnknownPersistedTodoCapabilityStillFailsStrictly() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "未知能力 Agent")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "unknown-persisted-capability",
            draft: .init(title: "拒绝未知能力", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let unknownPlan = """
        {"builtinCapabilities":["future_untrusted_capability"],"plugins":[],"requiresExecution":true,"selectedAtUnixMs":99,"selectionRevision":"local-capability-catalog-v1"}
        """
        try executeSQLite(
            url,
            sql: "UPDATE local_agent_todos SET execution_plan_json = '\(unknownPlan)' WHERE id = '\(todo.id)'"
        )

        do {
            _ = try await store.agentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                todoID: todo.id
            )
            XCTFail("unknown persisted capabilities must remain a hard failure")
        } catch let error as AgentGroupChatError {
            XCTAssertEqual(error.localizedDescription, "本地 Agent 群聊存储不可用：invalid Agent Todo execution plan")
        }
    }

    private func makeAgent(
        _ store: SQLiteAgentGroupChatStore,
        owner: String = "alice",
        name: String,
        canManageStaff: Bool = false,
        canAccessLocalProjects: Bool = false
    ) async throws -> LocalAgentProfile {
        try await store.createAgent(
            ownerUserID: owner,
            draft: .init(
                name: name,
                rolePrompt: "你是\(name)，只处理当前项目中明确交给你的工作。",
                modelConfigID: "model-1",
                defaultSkillIDs: LocalAgentPermission.normalized(
                    preserving: [],
                    canManageStaff: canManageStaff,
                    canAccessLocalProjects: canAccessLocalProjects
                )
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

    func testAgentAvatarPersistsUpdatesAndCanReturnToDefault() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let firstAvatar = Data(repeating: 0xA5, count: 1_024)
        let created = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "Avatar Agent",
                avatarData: firstAvatar,
                rolePrompt: "Use the selected avatar.",
                modelConfigID: "model-1"
            )
        )
        XCTAssertEqual(created.draft.avatarData, firstAvatar)
        let listed = try await store.listAgents(ownerUserID: "alice")
        XCTAssertEqual(listed.first?.draft.avatarData, firstAvatar)

        let secondAvatar = Data(repeating: 0x5A, count: 2_048)
        let updated = try await store.updateAgentProfile(
            ownerUserID: "alice",
            agentID: created.id,
            draft: .init(
                name: created.draft.name,
                avatarData: secondAvatar,
                rolePrompt: created.draft.rolePrompt,
                modelConfigID: created.draft.modelConfigID
            )
        )
        XCTAssertEqual(updated.draft.avatarData, secondAvatar)

        let reset = try await store.updateAgentProfile(
            ownerUserID: "alice",
            agentID: created.id,
            draft: .init(
                name: created.draft.name,
                rolePrompt: created.draft.rolePrompt,
                modelConfigID: created.draft.modelConfigID
            )
        )
        XCTAssertNil(reset.draft.avatarData)
    }

    func testListRoomsReturnsOnlyActiveRoomsForOwnerInRecentOrder() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let first = try await makeRoom(store, projectID: "project-1")
        let second = try await makeRoom(store, projectID: "project-2")
        _ = try await makeRoom(store, owner: "bob", projectID: "project-3")

        let rooms = try await store.listRooms(ownerUserID: "alice")

        XCTAssertEqual(Set(rooms.map(\.id)), Set([first.id, second.id]))
        XCTAssertEqual(rooms.map(\.ownerUserID), ["alice", "alice"])
    }

    func testHumanAgentDirectReusesConversationAndRoutesWithoutProjectTeam() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "私人助理")

        let first = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        let reopened = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )

        XCTAssertEqual(first.id, reopened.id)
        XCTAssertEqual(first.conversationKind, .humanAgentDirect)
        let teams = try await store.listRooms(ownerUserID: "alice")
        let directs = try await store.listDirectConversations(ownerUserID: "alice")
        XCTAssertTrue(teams.isEmpty)
        XCTAssertEqual(directs.map(\.id), [first.id])

        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: first.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "帮我创建一个项目团队"),
            limits: .init()
        )
        XCTAssertEqual(post.deliveries.map(\.targetAgentID), [agent.id])
    }

    func testMessageAttachmentsPersistLocallyAndRemainRoomScoped() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "视觉助手")
        let conversation = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        let data = Data("附件正文".utf8)

        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: conversation.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "",
                attachments: [
                    .init(
                        name: "说明.txt",
                        mimeType: "text/plain",
                        kind: .file,
                        origin: .pastedDocument,
                        data: data
                    ),
                ]
            ),
            limits: .init()
        )

        let attachment = try XCTUnwrap(post.message.attachmentItems.first)
        XCTAssertEqual(attachment.name, "说明.txt")
        XCTAssertEqual(attachment.size, data.count)
        let loadedPayload = try await store.messageAttachment(
            ownerUserID: "alice",
            roomID: conversation.id,
            messageID: post.message.id,
            attachmentID: attachment.id
        )
        let payload = try XCTUnwrap(loadedPayload)
        XCTAssertEqual(try Data(contentsOf: payload.localFileURL), data)

        let reopened = try SQLiteAgentGroupChatStore(databaseURL: url)
        let messages = try await reopened.listMessages(
            ownerUserID: "alice",
            roomID: conversation.id,
            limit: 20
        )
        XCTAssertEqual(messages.last?.attachmentItems, [attachment])
        let other = try await makeRoom(reopened, projectID: "project-other")
        let escaped = try await reopened.messageAttachment(
            ownerUserID: "alice",
            roomID: other.id,
            messageID: post.message.id,
            attachmentID: attachment.id
        )
        XCTAssertNil(escaped)
    }

    func testMessageAttachmentMetadataUsesOneBatchInsert() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "批量附件助手")
        let conversation = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        let drafts = (0..<12).map { index in
            ProjectAgentMessageAttachmentDraft(
                name: "附件-\(index).txt",
                mimeType: "text/plain",
                kind: .file,
                origin: .pastedDocument,
                data: Data("附件正文-\(index)".utf8)
            )
        }

        let before = await store.preparedStatementCountForTesting()
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: conversation.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "",
                attachments: drafts
            ),
            limits: .init()
        )
        let statementCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(statementCount, 8)
        XCTAssertEqual(post.message.attachmentItems.map(\.name), drafts.map(\.name))
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM project_agent_message_attachments WHERE message_id = '\(post.message.id)'"
        ), 12)
        for (draft, attachment) in zip(drafts, post.message.attachmentItems) {
            let loadedPayload = try await store.messageAttachment(
                ownerUserID: "alice",
                roomID: conversation.id,
                messageID: post.message.id,
                attachmentID: attachment.id
            )
            let payload = try XCTUnwrap(loadedPayload)
            XCTAssertEqual(try Data(contentsOf: payload.localFileURL), draft.data)
        }
    }

    func testAgentDirectPairIsOrderIndependentAndRoutesOnlyToPeer() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let firstAgent = try await makeAgent(store, name: "架构师")
        let secondAgent = try await makeAgent(store, name: "开发者")

        let first = try await store.openAgentDirect(
            ownerUserID: "alice",
            initiatingAgentID: firstAgent.id,
            targetAgentID: secondAgent.id
        )
        let reversed = try await store.openAgentDirect(
            ownerUserID: "alice",
            initiatingAgentID: secondAgent.id,
            targetAgentID: firstAgent.id
        )
        XCTAssertEqual(first.id, reversed.id)
        XCTAssertEqual(first.conversationKind, .agentAgentDirect)

        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: first.id,
            draft: .init(senderKind: .agent, senderID: firstAgent.id, content: "请检查接口"),
            limits: .init()
        )
        XCTAssertEqual(post.deliveries.map(\.targetAgentID), [secondAgent.id])

        do {
            _ = try await store.openAgentDirect(
                ownerUserID: "alice",
                initiatingAgentID: firstAgent.id,
                targetAgentID: firstAgent.id
            )
            XCTFail("Agent opened a direct conversation with itself")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .invalidField("targetAgentID"))
        }
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

    func testUnreadCursorIsStableMonotonicAndIsolatedPerAgent() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let firstAgent = try await makeAgent(store, name: "Agent A")
        let secondAgent = try await makeAgent(store, name: "Agent B")
        let room = try await makeRoom(store)
        for agent in [firstAgent, secondAgent] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: agent.draft.name)
            )
        }
        for content in ["消息一", "消息二", "消息三"] {
            _ = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(senderKind: .human, senderID: "alice", content: content),
                limits: .init()
            )
        }

        let fullPage = try await store.pageMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterMessageID: nil,
            limit: 10
        )
        XCTAssertEqual(fullPage.messages.count, 3)
        let recentPage = try await store.pageRecentMessages(
            ownerUserID: "alice",
            roomID: room.id,
            beforeMessageID: nil,
            limit: 2
        )
        XCTAssertEqual(recentPage.messages, Array(fullPage.messages.suffix(2)))
        XCTAssertTrue(recentPage.hasMore)
        let olderPage = try await store.pageRecentMessages(
            ownerUserID: "alice",
            roomID: room.id,
            beforeMessageID: try XCTUnwrap(recentPage.nextCursorMessageID),
            limit: 2
        )
        XCTAssertEqual(olderPage.messages, Array(fullPage.messages.prefix(1)))
        XCTAssertFalse(olderPage.hasMore)
        let firstUnread = try await store.listUnreadMessages(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: firstAgent.id,
            limit: 2
        )
        XCTAssertEqual(firstUnread.messages, Array(fullPage.messages.prefix(2)))
        XCTAssertTrue(firstUnread.hasMore)

        let secondMessage = try XCTUnwrap(firstUnread.messages.last)
        let cursor = try await store.markMessagesRead(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: firstAgent.id,
            throughMessageID: secondMessage.id,
            nowUnixMs: secondMessage.createdAtUnixMs + 1
        )
        XCTAssertEqual(cursor.messageID, secondMessage.id)
        let remaining = try await store.listUnreadMessages(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: firstAgent.id,
            limit: 10
        )
        XCTAssertEqual(remaining.messages, Array(fullPage.messages.dropFirst(2)))
        XCTAssertEqual(remaining.readThroughMessageID, secondMessage.id)

        let olderMessage = try XCTUnwrap(firstUnread.messages.first)
        let retriedOldCursor = try await store.markMessagesRead(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: firstAgent.id,
            throughMessageID: olderMessage.id,
            nowUnixMs: secondMessage.createdAtUnixMs + 2
        )
        XCTAssertEqual(retriedOldCursor.messageID, secondMessage.id)
        let secondAgentUnread = try await store.listUnreadMessages(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: secondAgent.id,
            limit: 10
        )
        XCTAssertEqual(secondAgentUnread.messages, fullPage.messages)

        let lastMessage = try XCTUnwrap(fullPage.messages.last)
        _ = try await store.markMessagesRead(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: firstAgent.id,
            throughMessageID: lastMessage.id,
            nowUnixMs: lastMessage.createdAtUnixMs + 3
        )
        _ = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .agent, senderID: firstAgent.id, content: "自己的回复"),
            limits: .init()
        )
        let afterOwnReply = try await store.listUnreadMessages(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: firstAgent.id,
            limit: 10
        )
        XCTAssertTrue(afterOwnReply.messages.isEmpty)
    }

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

    func testAgentProposalRequiresRunningIdentityAndHumanResolutionIsAtomic() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let proposer = try await makeAgent(store, name: "负责人", canManageStaff: true)
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: proposer.id,
            draft: .init(role: "负责人")
        )
        _ = try await store.setDefaultAgent(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: proposer.id
        )
        let incoming = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "需要补充测试角色",
                mentionedAgentIDs: [proposer.id]
            ),
            limits: .init()
        )
        let pendingDelivery = try XCTUnwrap(incoming.deliveries.first)
        do {
            _ = try await store.createAgentProposal(
                ownerUserID: "alice",
                roomID: room.id,
                proposerAgentID: proposer.id,
                sourceDeliveryID: pendingDelivery.id,
                requestKey: "call-before-claim",
                draft: .init(
                    name: "越权 Agent",
                    role: "观察员",
                    rolePrompt: "不应被创建。",
                    modelConfigID: "model-1"
                ),
                nowUnixMs: incoming.message.createdAtUnixMs + 1
            )
            XCTFail("A non-running delivery submitted an Agent proposal")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }
        let claimedDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: proposer.id,
            nowUnixMs: incoming.message.createdAtUnixMs + 1
        )
        let delivery = try XCTUnwrap(claimedDelivery)
        let draft = LocalAgentDraft(
            name: "测试 Agent",
            role: "测试工程师",
            responsibility: "验证项目",
            rolePrompt: "只处理测试工作。",
            modelConfigID: "inherit-current",
            rationale: "团队缺少测试能力"
        )
        let proposal = try await store.createAgentProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposerAgentID: proposer.id,
            sourceDeliveryID: delivery.id,
            requestKey: "call-1",
            draft: draft,
            nowUnixMs: incoming.message.createdAtUnixMs + 2
        )
        let replayed = try await store.createAgentProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposerAgentID: proposer.id,
            sourceDeliveryID: delivery.id,
            requestKey: "call-1",
            draft: draft,
            nowUnixMs: incoming.message.createdAtUnixMs + 3
        )
        XCTAssertEqual(replayed.id, proposal.id)
        do {
            _ = try await store.createAgentProposal(
                ownerUserID: "alice",
                roomID: room.id,
                proposerAgentID: proposer.id,
                sourceDeliveryID: delivery.id,
                requestKey: "call-1",
                draft: .init(
                    name: "冲突 Agent",
                    role: "测试工程师",
                    rolePrompt: "相同请求键不允许改变草案。",
                    modelConfigID: "model-1"
                ),
                nowUnixMs: incoming.message.createdAtUnixMs + 4
            )
            XCTFail("The same proposal request key accepted a different draft")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }

        let otherRoom = try await makeRoom(store, projectID: "project-2")
        let otherRoomProposals = try await store.listAgentProposals(
            ownerUserID: "alice",
            roomID: otherRoom.id
        )
        XCTAssertTrue(otherRoomProposals.isEmpty)
        do {
            _ = try await store.approveAgentProposal(
                ownerUserID: "alice",
                roomID: otherRoom.id,
                proposalID: proposal.id,
                nowUnixMs: incoming.message.createdAtUnixMs + 4
            )
            XCTFail("A proposal was approved through another room")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }

        let resolvedDraft = LocalAgentDraft(
            name: draft.name,
            role: draft.role,
            responsibility: draft.responsibility,
            rolePrompt: draft.rolePrompt,
            modelConfigID: "model-1",
            thinkingLevel: "medium",
            professionKey: draft.professionKey,
            rationale: draft.rationale
        )
        let approval = try await store.approveAgentProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposalID: proposal.id,
            nowUnixMs: incoming.message.createdAtUnixMs + 4,
            resolvedDraft: resolvedDraft
        )
        XCTAssertEqual(approval.proposal.status, .approved)
        XCTAssertEqual(approval.proposal.draft.modelConfigID, "model-1")
        XCTAssertEqual(approval.agent.draft.modelConfigID, "model-1")
        XCTAssertEqual(approval.proposal.draft.thinkingLevel, "medium")
        XCTAssertEqual(approval.agent.draft.thinkingLevel, "medium")
        XCTAssertEqual(approval.proposal.createdAgentID, approval.agent.id)
        XCTAssertEqual(approval.member?.agentID, approval.agent.id)
        XCTAssertEqual(approval.member?.draft.role, draft.role)
        let members = try await store.listMembers(ownerUserID: "alice", roomID: room.id)
        XCTAssertEqual(Set(members.map(\.agentID)), Set([proposer.id, approval.agent.id]))
        do {
            _ = try await store.approveAgentProposal(
                ownerUserID: "alice",
                roomID: room.id,
                proposalID: proposal.id,
                nowUnixMs: incoming.message.createdAtUnixMs + 5
            )
            XCTFail("An approved proposal was processed twice")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }
        do {
            _ = try await store.rejectAgentProposal(
                ownerUserID: "alice",
                roomID: room.id,
                proposalID: proposal.id,
                nowUnixMs: incoming.message.createdAtUnixMs + 5
            )
            XCTFail("An approved proposal was rejected")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }

        let rejectedDraft = LocalAgentDraft(
            name: "多余 Agent",
            role: "观察员",
            rolePrompt: "只观察。",
            modelConfigID: "model-1"
        )
        let rejectedProposal = try await store.createAgentProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposerAgentID: proposer.id,
            sourceDeliveryID: delivery.id,
            requestKey: "call-2",
            draft: rejectedDraft,
            nowUnixMs: incoming.message.createdAtUnixMs + 5
        )
        let rejected = try await store.rejectAgentProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposalID: rejectedProposal.id,
            nowUnixMs: incoming.message.createdAtUnixMs + 6
        )
        XCTAssertEqual(rejected.status, .rejected)
        XCTAssertNil(rejected.createdAgentID)
        do {
            _ = try await store.rejectAgentProposal(
                ownerUserID: "alice",
                roomID: room.id,
                proposalID: rejectedProposal.id,
                nowUnixMs: incoming.message.createdAtUnixMs + 7
            )
            XCTFail("A rejected proposal was processed twice")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }
        let pending = try await store.listAgentProposals(
            ownerUserID: "alice",
            roomID: room.id,
            status: .pending
        )
        XCTAssertTrue(pending.isEmpty)
    }

    func testStaffPermissionGatesHireAndRemovalProposalAndHumanRemovalPreservesProfile() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await makeAgent(store, name: "负责人", canManageStaff: true)
        let ordinary = try await makeAgent(store, name: "普通成员")
        let target = try await makeAgent(store, name: "待移出成员")
        let room = try await makeRoom(store)
        for agent in [manager, ordinary, target] {
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
            agentID: target.id
        )
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "检查团队配置",
                mentionedAgentIDs: [manager.id, ordinary.id]
            ),
            limits: .init()
        )
        let claimedManagerDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: manager.id,
            nowUnixMs: post.message.createdAtUnixMs + 1
        )
        let managerDelivery = try XCTUnwrap(claimedManagerDelivery)
        let claimedOrdinaryDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: ordinary.id,
            nowUnixMs: post.message.createdAtUnixMs + 1
        )
        let ordinaryDelivery = try XCTUnwrap(claimedOrdinaryDelivery)
        let draft = LocalAgentRemovalProposalDraft(
            targetAgentID: target.id,
            reason: "职责已经由现有成员稳定覆盖",
            handoffPlan: "文档和未完成事项交给负责人"
        )

        do {
            _ = try await store.createAgentRemovalProposal(
                ownerUserID: "alice",
                roomID: room.id,
                proposerAgentID: ordinary.id,
                sourceDeliveryID: ordinaryDelivery.id,
                requestKey: "ordinary-remove",
                draft: draft,
                nowUnixMs: post.message.createdAtUnixMs + 2
            )
            XCTFail("An Agent without staffing permission submitted a removal proposal")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }

        let proposal = try await store.createAgentRemovalProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposerAgentID: manager.id,
            sourceDeliveryID: managerDelivery.id,
            requestKey: "manager-remove",
            draft: draft,
            nowUnixMs: post.message.createdAtUnixMs + 2
        )
        let replay = try await store.createAgentRemovalProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposerAgentID: manager.id,
            sourceDeliveryID: managerDelivery.id,
            requestKey: "manager-remove",
            draft: draft,
            nowUnixMs: post.message.createdAtUnixMs + 3
        )
        XCTAssertEqual(replay.id, proposal.id)

        let approved = try await store.approveAgentRemovalProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposalID: proposal.id,
            nowUnixMs: post.message.createdAtUnixMs + 4
        )
        XCTAssertEqual(approved.status, .approved)
        let members = try await store.listMembers(ownerUserID: "alice", roomID: room.id)
        XCTAssertFalse(members.contains(where: { $0.agentID == target.id }))
        let profiles = try await store.listAgents(ownerUserID: "alice", includeArchived: false)
        XCTAssertTrue(profiles.contains(where: { $0.id == target.id }))
        let reloadedRoom = try await store.activeRoom(
            ownerUserID: "alice",
            projectID: room.projectID
        )
        let updatedRoom = try XCTUnwrap(reloadedRoom)
        XCTAssertNotEqual(updatedRoom.defaultAgentID, target.id)
        XCTAssertTrue(members.contains(where: { $0.agentID == updatedRoom.defaultAgentID }))

        let restored = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: target.id,
            draft: .init(role: "重新加入")
        )
        XCTAssertEqual(restored.status, .active)
    }

    func testExistingAgentMembershipProposalFromDirectChatSupportsMultipleTeamsAndAssignsManager() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let proposer = try await makeAgent(store, name: "管家", canManageStaff: true)
        let projectManager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "玄德",
                rolePrompt: "负责项目管理。",
                modelConfigID: "model-1",
                professionKey: "project_manager"
            )
        )
        let targetTeam = try await makeRoom(store, projectID: "project-target")
        let otherTeam = try await makeRoom(store, projectID: "project-other")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: otherTeam.id,
            agentID: projectManager.id,
            draft: .init(role: "项目经理")
        )
        let direct = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: proposer.id
        )
        let incoming = try await store.postMessage(
            ownerUserID: "alice",
            roomID: direct.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "把玄德加入目标团队"
            ),
            limits: .init()
        )
        let claimedDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: proposer.id,
            nowUnixMs: incoming.message.createdAtUnixMs + 1
        )
        let claimed = try XCTUnwrap(claimedDelivery)
        let draft = LocalAgentMembershipProposalDraft(
            targetTeamRoomID: targetTeam.id,
            targetAgentID: projectManager.id,
            role: "项目经理",
            responsibility: "负责排期、依赖和交付"
        )
        let proposal = try await store.createMembershipProposal(
            ownerUserID: "alice",
            sourceRoomID: direct.id,
            proposerAgentID: proposer.id,
            sourceDeliveryID: claimed.id,
            requestKey: "invite-existing",
            draft: draft,
            nowUnixMs: incoming.message.createdAtUnixMs + 2
        )
        let replay = try await store.createMembershipProposal(
            ownerUserID: "alice",
            sourceRoomID: direct.id,
            proposerAgentID: proposer.id,
            sourceDeliveryID: claimed.id,
            requestKey: "invite-existing",
            draft: draft,
            nowUnixMs: incoming.message.createdAtUnixMs + 3
        )
        XCTAssertEqual(replay.id, proposal.id)

        let approval = try await store.approveMembershipProposal(
            ownerUserID: "alice",
            sourceRoomID: direct.id,
            proposalID: proposal.id,
            nowUnixMs: incoming.message.createdAtUnixMs + 4
        )
        XCTAssertEqual(approval.proposal.status, .approved)
        XCTAssertEqual(approval.member.agentID, projectManager.id)
        XCTAssertEqual(approval.room.projectManagerAgentID, projectManager.id)
        let targetMembers = try await store.listMembers(
            ownerUserID: "alice",
            roomID: targetTeam.id
        )
        let otherMembers = try await store.listMembers(
            ownerUserID: "alice",
            roomID: otherTeam.id
        )
        XCTAssertTrue(targetMembers.contains { $0.agentID == projectManager.id })
        XCTAssertTrue(otherMembers.contains { $0.agentID == projectManager.id })
        do {
            _ = try await store.approveMembershipProposal(
                ownerUserID: "alice",
                sourceRoomID: direct.id,
                proposalID: proposal.id,
                nowUnixMs: incoming.message.createdAtUnixMs + 5
            )
            XCTFail("An approved membership proposal was processed twice")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }
    }

    func testEveryTeamAgentCanSubmitIdempotentProjectProposalForHumanResolution() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let projectAgent = try await makeAgent(store, name: "项目负责人")
        let ordinary = try await makeAgent(store, name: "普通成员")
        let room = try await makeRoom(store)
        for agent in [projectAgent, ordinary] {
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
                content: "帮我规划一个新项目",
                mentionedAgentIDs: [projectAgent.id, ordinary.id]
            ),
            limits: .init()
        )
        let claimedProjectDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: projectAgent.id,
            nowUnixMs: post.message.createdAtUnixMs + 1
        )
        let projectDelivery = try XCTUnwrap(claimedProjectDelivery)
        let claimedOrdinaryDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: ordinary.id,
            nowUnixMs: post.message.createdAtUnixMs + 1
        )
        let ordinaryDelivery = try XCTUnwrap(claimedOrdinaryDelivery)
        let draft = LocalProjectCreationProposalDraft(
            name: "新项目",
            description: "由项目负责人整理的项目说明"
        )
        let ordinaryProposal = try await store.createProjectProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposerAgentID: ordinary.id,
            sourceDeliveryID: ordinaryDelivery.id,
            requestKey: "ordinary-call",
            draft: .init(name: "普通成员提案", description: "同样等待 Human 确认"),
            nowUnixMs: post.message.createdAtUnixMs + 2
        )
        XCTAssertEqual(ordinaryProposal.status, .pending)

        let proposal = try await store.createProjectProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposerAgentID: projectAgent.id,
            sourceDeliveryID: projectDelivery.id,
            requestKey: "project-call",
            draft: draft,
            nowUnixMs: post.message.createdAtUnixMs + 2
        )
        let replay = try await store.createProjectProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposerAgentID: projectAgent.id,
            sourceDeliveryID: projectDelivery.id,
            requestKey: "project-call",
            draft: draft,
            nowUnixMs: post.message.createdAtUnixMs + 3
        )
        XCTAssertEqual(replay.id, proposal.id)
        let pending = try await store.listProjectProposals(
            ownerUserID: "alice",
            roomID: room.id,
            status: .pending
        )
        XCTAssertEqual(Set(pending.map(\.id)), Set([ordinaryProposal.id, proposal.id]))
        let approved = try await store.approveProjectProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposalID: proposal.id,
            createdProjectID: "project-created",
            nowUnixMs: post.message.createdAtUnixMs + 4
        )
        XCTAssertEqual(approved.status, .approved)
        XCTAssertEqual(approved.createdProjectID, "project-created")

        let updated = try await store.updateAgentProfile(
            ownerUserID: "alice",
            agentID: projectAgent.id,
            draft: .init(
                name: "项目负责人",
                rolePrompt: projectAgent.draft.rolePrompt,
                modelConfigID: "model-2"
            )
        )
        XCTAssertEqual(updated.draft.modelConfigID, "model-2")
        XCTAssertTrue(updated.draft.defaultSkillIDs.isEmpty)
    }

    func testTeamToolUsesOpaqueSingleChoiceAndProgramPassesThroughProjectID() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let localRoot = url.deletingLastPathComponent()
        let importedDirectory = localRoot.appendingPathComponent("import-target", isDirectory: true)
        try FileManager.default.createDirectory(
            at: importedDirectory,
            withIntermediateDirectories: true
        )
        let connectorStateURL = localRoot.appendingPathComponent("connector.json")
        var connectorState = NativeConnectorPersistentState.empty
        connectorState.user = .init(
            id: "alice",
            username: "alice",
            displayName: nil,
            role: "user"
        )
        connectorState.deviceID = "device"
        connectorState.workspaces = [.init(
            id: "workspace-1",
            alias: "workspace",
            absoluteRoot: localRoot.path,
            fingerprint: "fingerprint"
        )]
        try NativeConnectorStateStore(stateURL: connectorStateURL).save(connectorState)
        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: connectorStateURL
            ),
            ticketProvider: NoNetworkTicketProvider()
        )
        let projectsService = NativeLocalProjectsService(
            connector: connector,
            databaseURL: localRoot.appendingPathComponent("projects.db")
        )
        let agent = try await makeAgent(
            store,
            name: "团队负责人",
            canAccessLocalProjects: true
        )
        XCTAssertEqual(agent.draft.professionKey, "general_member")
        let room = try await makeRoom(store, projectID: "source-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "团队负责人")
        )
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "为设计项目创建团队",
                mentionedAgentIDs: [agent.id]
            ),
            limits: .init()
        )
        let claimed = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: post.message.createdAtUnixMs + 1
        )
        let delivery = try XCTUnwrap(claimed)
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: room.projectID,
            roomID: room.id,
            agentID: agent.id,
            deliveryID: delivery.id,
            triggerMessageID: post.message.id,
            rootMessageID: post.message.rootMessageID,
            runID: "team-tool-run",
            hopCount: delivery.hopCount
        )
        let secretProjectID = "secret-project-id-never-given-to-model"
        let target = LocalProjectRecord(
            id: secretProjectID,
            ownerUserID: "alice",
            draft: .init(
                name: "设计系统",
                workspaceID: "workspace-1",
                relativeRoot: "design"
            ),
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
        let productSkillSession = ProductToolSkillSession()
        let provider = LocalAgentProjectToolProvider(
            store: store,
            projects: [target],
            projectsService: projectsService,
            context: context,
            productSkillSession: productSkillSession,
            now: { post.message.createdAtUnixMs + 2 }
        )
        let definitions = try await provider.definitions()
        XCTAssertEqual(definitions.map(\.name), [
            "project_catalog",
            "team_propose_existing",
            "team_propose_new_project",
            "team_propose_import_directory",
        ])
        let coverage = ToolSkillCoverageCatalog.product.audit(definitions.map {
            .init(
                providerID: $0.providerID,
                toolName: $0.name,
                skillBindingID: $0.skillBindingID
            )
        })
        XCTAssertTrue(coverage.isComplete)
        XCTAssertEqual(coverage.coveredTools, 4)
        XCTAssertFalse(definitions.map(\.name).contains("team_propose"))
        for definition in definitions {
            XCTAssertTrue(
                definition.description.contains("不要求")
                    && definition.description.contains("项目经理"),
                definition.name
            )
        }
        let catalog = try await provider.execute(.init(
            id: "project-catalog-call",
            name: "project_catalog",
            arguments: "{}"
        ))
        XCTAssertTrue(catalog.content.contains(#""total_project_count":1"#))
        XCTAssertTrue(catalog.content.contains(#""available_for_team_count":1"#))
        XCTAssertTrue(catalog.content.contains("product-skill:chatos-project-team-setup"))
        let gatedProposal = try await provider.execute(.init(
            id: "proposal-before-skill-activation",
            name: "team_propose_new_project",
            arguments: #"{"project_name":"不应创建","project_type":"software_development","team_name":"未激活"}"#
        ))
        XCTAssertTrue(gatedProposal.isError)
        XCTAssertTrue(gatedProposal.content.contains("agent_skill_activate"))
        _ = try await productSkillSession.activate(
            skillRef: "product-skill:chatos-project-team-setup"
        )
        let teamDefinition = try XCTUnwrap(definitions.first {
            $0.name == LocalAgentProjectToolProvider.proposeExistingTeamToolName
        })
        let schema = String(decoding: teamDefinition.schema, as: UTF8.self)
        XCTAssertTrue(schema.contains("设计系统"))
        XCTAssertTrue(schema.contains("existing_1"))
        XCTAssertFalse(schema.contains(secretProjectID))
        XCTAssertFalse(schema.contains("absolute_path"))
        XCTAssertFalse(schema.contains("project_name"))
        let newProjectDefinition = try XCTUnwrap(definitions.first {
            $0.name == LocalAgentProjectToolProvider.proposeNewProjectTeamToolName
        })
        let newProjectSchema = String(
            decoding: newProjectDefinition.schema,
            as: UTF8.self
        )
        XCTAssertFalse(newProjectSchema.contains("absolute_path"))
        XCTAssertFalse(newProjectSchema.contains("project_option"))
        XCTAssertThrowsError(try AgentSchemaValidator.validate(
            arguments: #"{"project_name":"错误混参","project_type":"software_development","team_name":"团队","absolute_path":"/"}"#,
            schema: newProjectDefinition.schema
        ))
        let importDefinition = try XCTUnwrap(definitions.first {
            $0.name == LocalAgentProjectToolProvider.proposeImportedDirectoryTeamToolName
        })
        XCTAssertTrue(importDefinition.description.contains("GitHub/GitLab URL"))
        let importSchema = String(decoding: importDefinition.schema, as: UTF8.self)
        XCTAssertTrue(importSchema.contains("absolute_path"))
        XCTAssertFalse(importSchema.contains("project_option"))
        let rejectedRepositoryURL = try await provider.execute(.init(
            id: "remote-repository-must-not-be-imported",
            name: "team_propose_import_directory",
            arguments: #"{"absolute_path":"https://github.com/TencentCloud/CubeSandbox.git","project_type":"software_development","team_name":"错误团队"}"#
        ))
        XCTAssertTrue(rejectedRepositoryURL.isError)
        XCTAssertTrue(rejectedRepositoryURL.content.contains("不是本机路径"))
        XCTAssertTrue(rejectedRepositoryURL.content.contains("不得猜测或编造 /"))

        let newProjectResult = try await provider.execute(.init(
            id: "new-project-team-proposal-call",
            name: "team_propose_new_project",
            arguments: #"{"project_name":"新建调研项目","project_description":"独立新项目","project_type":"software_development","team_name":"新项目团队","team_goal":"完成调研"}"#
        ))
        XCTAssertFalse(newProjectResult.isError)
        XCTAssertTrue(newProjectResult.content.contains(#""creates_new_project":true"#))

        let importResult = try await provider.execute(.init(
            id: "import-directory-team-proposal-call",
            name: "team_propose_import_directory",
            arguments: try toolArguments([
                "absolute_path": importedDirectory.path,
                "project_type": "software_development",
                "team_name": "现有目录团队",
                "team_goal": "在原目录工作",
            ])
        ))
        XCTAssertFalse(importResult.isError)
        XCTAssertTrue(importResult.content.contains(#""creates_new_project":true"#))

        let result = try await provider.execute(.init(
            id: "team-proposal-call",
            name: "team_propose_existing",
            arguments: #"{"project_option":"existing_1","team_name":"设计团队","team_goal":"完成产品设计"}"#
        ))
        XCTAssertFalse(result.content.contains(secretProjectID))
        let proposals = try await store.listTeamProposals(
            ownerUserID: "alice",
            sourceRoomID: room.id,
            status: .pending
        )
        let proposal = try XCTUnwrap(proposals.first(where: {
            $0.requestKey == "team-proposal-call"
        }))
        XCTAssertEqual(proposal.draft.existingProjectID, secretProjectID)
        let newProjectProposal = try XCTUnwrap(proposals.first(where: {
            $0.requestKey == "new-project-team-proposal-call"
        }))
        XCTAssertEqual(newProjectProposal.draft.newProjectName, "新建调研项目")
        XCTAssertNil(newProjectProposal.draft.importedProjectDraft)
        let importedProposal = try XCTUnwrap(proposals.first(where: {
            $0.requestKey == "import-directory-team-proposal-call"
        }))
        XCTAssertEqual(importedProposal.draft.importedProjectAbsolutePath, importedDirectory.path)
        XCTAssertEqual(importedProposal.draft.importedProjectDraft?.relativeRoot, "import-target")
        XCTAssertNil(importedProposal.draft.newProjectName)

        // The proposal was submitted from a running communication cycle. Resolve that source
        // delivery before the Human decision so the follow-up delivery can be claimed normally.
        _ = try await store.failDelivery(
            ownerUserID: "alice",
            deliveryID: delivery.id,
            error: "test source cycle finished",
            nowUnixMs: post.message.createdAtUnixMs + 3
        )

        do {
            _ = try await store.approveTeamProposal(
                ownerUserID: "alice",
                sourceRoomID: room.id,
                proposalID: proposal.id,
                resolvedProjectID: "wrong-project-id",
                nowUnixMs: post.message.createdAtUnixMs + 3
            )
            XCTFail("Human approval changed the program-resolved project id")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }
        let approval = try await store.approveTeamProposal(
            ownerUserID: "alice",
            sourceRoomID: room.id,
            proposalID: proposal.id,
            resolvedProjectID: secretProjectID,
            nowUnixMs: post.message.createdAtUnixMs + 3
        )
        XCTAssertEqual(approval.room.projectID, secretProjectID)
        let claimedResolutionDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: post.message.createdAtUnixMs + 4
        )
        let resolutionDelivery = try XCTUnwrap(claimedResolutionDelivery)
        XCTAssertEqual(resolutionDelivery.roomID, room.id)
        XCTAssertEqual(resolutionDelivery.targetAgentID, agent.id)
        XCTAssertEqual(resolutionDelivery.triggerKind, .mention)
        let loadedResolutionMessage = try await store.message(
            ownerUserID: "alice",
            roomID: room.id,
            messageID: resolutionDelivery.messageID
        )
        let resolutionMessage = try XCTUnwrap(loadedResolutionMessage)
        XCTAssertEqual(resolutionMessage.senderKind, .system)
        XCTAssertEqual(resolutionMessage.causationID, proposal.id)
        XCTAssertEqual(resolutionMessage.mentionedAgentIDs, [agent.id])
        XCTAssertTrue(resolutionMessage.content.contains("Human 已批准"))
        XCTAssertTrue(resolutionMessage.content.contains("不要等待 Human 再次提醒"))
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

    func testMentionMembershipValidationUsesOneQueryForTheWholeMessage() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let room = try await makeRoom(store)
        var mentionedAgentIDs: [String] = []
        for index in 0..<12 {
            let agent = try await makeAgent(store, name: "批量校验成员 \(index)")
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "成员")
            )
            mentionedAgentIDs.append(agent.id)
        }
        let outsider = try await makeAgent(store, name: "批量校验外部成员")
        mentionedAgentIDs.append(outsider.id)

        let countBefore = await store.preparedStatementCountForTesting()
        do {
            _ = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: "alice",
                    content: "一次校验全部提及成员",
                    mentionedAgentIDs: mentionedAgentIDs
                ),
                limits: .init()
            )
            XCTFail("Non-member mention was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .notMember)
        }
        let queryCount = await store.preparedStatementCountForTesting() - countBefore
        let messages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 20
        )

        // BEGIN + room + active-members snapshot + ROLLBACK. The count is independent of the
        // number of mentioned Agents; the old path added one SELECT for every identifier.
        XCTAssertEqual(queryCount, 4)
        XCTAssertTrue(messages.isEmpty)

        let validMentionIDs = Array(mentionedAgentIDs.dropLast())
        let validCountBefore = await store.preparedStatementCountForTesting()
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "批量创建投递",
                mentionedAgentIDs: validMentionIDs
            ),
            limits: .init(maximumAgentRunsPerRootMessage: 32)
        )
        let validQueryCount = await store.preparedStatementCountForTesting() - validCountBefore

        // BEGIN + room + active-members snapshot + message + batched mentions + delivery count
        // + batched deliveries + COMMIT. The write count no longer grows with recipients.
        XCTAssertEqual(validQueryCount, 8)
        XCTAssertEqual(post.deliveries.count, validMentionIDs.count)
        XCTAssertEqual(post.deliveries.map(\.targetAgentID), validMentionIDs)
        XCTAssertTrue(post.deliveries.allSatisfy { $0.status == .pending && $0.attempt == 0 })
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

    func testTodoRunSummaryProjectsCommittedPathsWithoutReturningManagerRuns() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "摘要执行者")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "summary-todo",
            draft: .init(title: "生成轻量摘要", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let deliveryValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        let delivery = try XCTUnwrap(deliveryValue)
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: room.projectID,
            roomID: room.id,
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
            messages: [.init(role: .system, content: "large history is not needed here")]
        )
        checkpoint.id = runID
        checkpoint.status = .completed
        checkpoint.receipts = [
            "write": .init(#"{"result":{"committed_paths":["Sources/B.swift","Sources/A.swift"]}}"#),
            "duplicate": .init(#"{"committed_paths":["Sources/A.swift"]}"#),
            "failed": .failure(#"{"committed_paths":["Sources/Failed.swift"]}"#),
        ]
        try await store.saveRun(try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: agent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: 101,
            updatedAtUnixMs: 102
        ))

        let summaries = try await store.listLatestTodoRunSummaries(
            ownerUserID: "alice",
            roomID: room.id,
            limit: 500
        )

        XCTAssertEqual(summaries, [
            .init(
                todoID: todo.id,
                runID: runID,
                status: .completed,
                receiptCount: 3,
                committedPaths: ["Sources/A.swift", "Sources/B.swift"],
                updatedAtUnixMs: 102
            ),
        ])
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

    func testStopOutstandingDeliveriesLoadsAllRunsWithOneQuery() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let room = try await makeRoom(store, projectID: "stop-batch-project")
        var agents: [LocalAgentProfile] = []
        for index in 0..<12 {
            let agent = try await makeAgent(store, name: "停止批量 Agent \(index)")
            agents.append(agent)
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "执行者")
            )
        }
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "批量停止",
                mentionedAgentIDs: agents.map(\.id)
            ),
            limits: .init(maximumAgentRunsPerRootMessage: 20)
        )
        for agent in agents {
            let claimed = try await store.claimNextDelivery(
                ownerUserID: "alice",
                agentID: agent.id,
                nowUnixMs: post.message.createdAtUnixMs + 1
            )
            let delivery = try XCTUnwrap(claimed)
            let runID = UUID()
            let context = try LocalAgentChatRunContext(
                ownerUserID: "alice",
                projectID: "stop-batch-project",
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
            checkpoint.status = .paused
            try await store.saveRun(LocalAgentGroupChatRun(
                id: runID,
                context: context,
                modelConfigID: agent.draft.modelConfigID,
                policy: .init(),
                checkpoint: checkpoint,
                createdAtUnixMs: post.message.createdAtUnixMs + 1,
                updatedAtUnixMs: post.message.createdAtUnixMs + 1
            ))
        }

        let before = await store.preparedStatementCountForTesting()
        let stopped = try await store.stopOutstandingDeliveries(
            ownerUserID: "alice",
            roomID: room.id,
            reason: "用户批量停止全部 Agent。",
            nowUnixMs: post.message.createdAtUnixMs + 2
        )
        let statementCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(stopped, 12)
        XCTAssertEqual(statementCount, 7)
        let runs = try await store.listRoomRuns(
            ownerUserID: "alice",
            roomID: room.id,
            limit: 20
        )
        XCTAssertEqual(runs.count, 12)
        XCTAssertTrue(runs.allSatisfy { $0.checkpoint.status == .failed })
        XCTAssertTrue(runs.allSatisfy { $0.checkpoint.stopReason == "用户批量停止全部 Agent。" })
    }

    func testAgentProfileAndProjectMembershipUpdateTogether() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "旧名称")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "旧角色", pluginAllowlist: ["plugin.old"])
        )

        let result = try await store.updateAgentMembership(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            profileDraft: .init(
                name: "新名称",
                description: "新职责",
                rolePrompt: "使用新的角色指令。",
                modelConfigID: "model-2",
                thinkingLevel: "high",
                defaultPluginIDs: ["plugin.new"],
                defaultSkillIDs: ["skill.keep"]
            ),
            memberDraft: .init(
                role: "新角色",
                responsibility: "新职责",
                pluginAllowlist: ["plugin.new"]
            )
        )

        XCTAssertEqual(result.profile.draft.name, "新名称")
        XCTAssertEqual(result.profile.draft.modelConfigID, "model-2")
        XCTAssertEqual(result.profile.draft.thinkingLevel, "high")
        XCTAssertEqual(result.profile.draft.defaultSkillIDs, ["skill.keep"])
        XCTAssertEqual(result.member.draft.role, "新角色")
        XCTAssertEqual(result.member.draft.pluginAllowlist, ["plugin.new"])
        let profiles = try await store.listAgents(ownerUserID: "alice", includeArchived: false)
        let members = try await store.listMembers(ownerUserID: "alice", roomID: room.id)
        XCTAssertEqual(profiles.first(where: { $0.id == agent.id }), result.profile)
        XCTAssertEqual(members.first(where: { $0.agentID == agent.id }), result.member)
    }

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

    func testTeamTodoDependenciesGateCrossAgentParallelExecutionAndRejectCycles() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let first = try await makeAgent(store, name: "前置负责人")
        let second = try await makeAgent(store, name: "后置负责人")
        let third = try await makeAgent(store, name: "等待负责人")
        let room = try await makeRoom(store, projectID: "dependency-project")
        for agent in [first, second, third] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: agent.draft.name)
            )
        }
        let prerequisite = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            requestKey: "dependency-prerequisite",
            draft: .init(title: "先完成接口", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let dependent = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: second.id,
            requestKey: "dependency-dependent",
            draft: .init(
                title: "再实现客户端",
                teamRoomID: room.id,
                dependencies: [.init(
                    prerequisiteTodoID: prerequisite.id,
                    prerequisiteAgentID: first.id
                )]
            ),
            nowUnixMs: 101
        )

        let firstWave = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: 102
        )
        XCTAssertEqual(firstWave.count, 1)
        XCTAssertTrue(firstWave[0].deduplicationKey.hasSuffix(prerequisite.id))
        XCTAssertFalse(firstWave[0].deduplicationKey.hasSuffix(dependent.id))

        let claimedValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: first.id,
            nowUnixMs: 103
        )
        let claimed = try XCTUnwrap(claimedValue)
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            todoID: prerequisite.id,
            update: .init(status: .completed, result: "接口已完成"),
            nowUnixMs: 104
        )
        _ = try await store.completeHeartbeatDelivery(
            ownerUserID: "alice",
            deliveryID: claimed.id,
            nowUnixMs: 105
        )
        let secondWave = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: 106
        )
        XCTAssertEqual(secondWave.count, 1)
        XCTAssertTrue(secondWave[0].deduplicationKey.hasSuffix(dependent.id))

        let blockedPrerequisite = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            requestKey: "blocked-prerequisite",
            draft: .init(title: "阻塞的前置", teamRoomID: room.id),
            nowUnixMs: 107
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            todoID: blockedPrerequisite.id,
            update: .init(status: .inProgress),
            nowUnixMs: 108
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            todoID: blockedPrerequisite.id,
            update: .init(status: .blocked, blockedReason: "等待外部输入"),
            nowUnixMs: 109
        )
        let waitingOnBlocked = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: third.id,
            requestKey: "waiting-on-blocked",
            draft: .init(
                title: "不能越过阻塞前置",
                teamRoomID: room.id,
                dependencies: [.init(
                    prerequisiteTodoID: blockedPrerequisite.id,
                    prerequisiteAgentID: first.id
                )]
            ),
            nowUnixMs: 109
        )
        let cancelledPrerequisite = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            requestKey: "cancelled-prerequisite",
            draft: .init(title: "取消的前置", teamRoomID: room.id),
            nowUnixMs: 110
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            todoID: cancelledPrerequisite.id,
            update: .init(status: .cancelled),
            nowUnixMs: 111
        )
        let waitingOnCancelled = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: third.id,
            requestKey: "waiting-on-cancelled",
            draft: .init(
                title: "不能越过取消前置",
                teamRoomID: room.id,
                dependencies: [.init(
                    prerequisiteTodoID: cancelledPrerequisite.id,
                    prerequisiteAgentID: first.id
                )]
            ),
            nowUnixMs: 112
        )
        let blockedWave = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: 113
        )
        XCTAssertFalse(blockedWave.contains {
            $0.deduplicationKey.hasSuffix(waitingOnBlocked.id)
                || $0.deduplicationKey.hasSuffix(waitingOnCancelled.id)
        })

        do {
            _ = try await store.setAgentTodoDependencies(
                ownerUserID: "alice",
                agentID: first.id,
                todoID: prerequisite.id,
                dependencies: [.init(
                    prerequisiteTodoID: dependent.id,
                    prerequisiteAgentID: second.id
                )],
                nowUnixMs: 114
            )
            XCTFail("A dependency cycle was accepted")
        } catch {
            XCTAssertEqual(
                error as? AgentGroupChatError,
                .invalidField("todoDependencyCycle")
            )
        }
    }

    func testManagerExplicitlyStartsOnlyHighestPriorityReadyTodoAndCancellationStopsExecutor() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let assignee = try await makeAgent(store, name: "执行者")
        let prerequisiteOwner = try await makeAgent(store, name: "前置负责人")
        let room = try await makeRoom(store, projectID: "explicit-scheduling-project")
        for agent in [assignee, prerequisiteOwner] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: agent.draft.name)
            )
        }
        let prerequisite = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: prerequisiteOwner.id,
            requestKey: "explicit-prerequisite",
            draft: .init(title: "尚未完成的前置", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let blockedHighPriority = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            requestKey: "explicit-blocked",
            draft: .init(
                title: "有前置的高优先级任务",
                priority: 100,
                teamRoomID: room.id,
                dependencies: [.init(
                    prerequisiteTodoID: prerequisite.id,
                    prerequisiteAgentID: prerequisiteOwner.id
                )]
            ),
            nowUnixMs: 101
        )
        let ready = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            requestKey: "explicit-ready",
            draft: .init(title: "可立即执行", priority: 60, teamRoomID: room.id),
            nowUnixMs: 102
        )

        let startedDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            nowUnixMs: 103
        )
        let firstDelivery = try XCTUnwrap(startedDelivery)
        let todoLookupCountBefore = await store.preparedStatementCountForTesting()
        let selected = try await store.todoForDelivery(
            ownerUserID: "alice",
            deliveryID: firstDelivery.id
        )
        let todoLookupCount = await store.preparedStatementCountForTesting()
            - todoLookupCountBefore
        XCTAssertEqual(selected?.id, ready.id)
        XCTAssertEqual(todoLookupCount, 1)
        let stillBlocked = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            todoID: blockedHighPriority.id
        )
        XCTAssertEqual(stillBlocked?.status, .pending)
        let secondDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            nowUnixMs: 104
        )
        XCTAssertNil(secondDelivery, "同一 Agent 已有 executor 时不能再启动第二个 Todo")

        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            todoID: ready.id,
            update: .init(status: .cancelled),
            nowUnixMs: 105
        )
        let cancelledDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: firstDelivery.id
        )
        XCTAssertEqual(cancelledDelivery?.status, .cancelled)
        do {
            _ = try await store.updateAgentTodo(
                ownerUserID: "alice",
                agentID: assignee.id,
                todoID: ready.id,
                update: .init(status: .completed, result: "不应写入"),
                nowUnixMs: 106
            )
            XCTFail("Cancelled executor completed its Todo")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }
        let noReadyDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            nowUnixMs: 107
        )
        XCTAssertNil(noReadyDelivery, "前置未完成的 Todo 不能被启动")
    }

    func testTeamAssetsRequireProjectManagerAndTodoKeepsStartRevisionSnapshot() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护团队资产与任务板。",
                modelConfigID: "model-1",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(store, name: "工程师")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "asset-snapshot-project",
            draft: .init(name: "资产快照团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: worker.id,
            draft: .init(role: "工程师")
        )

        do {
            _ = try await store.upsertTeamAsset(
                ownerUserID: "alice",
                teamRoomID: room.id,
                assetID: nil,
                editorAgentID: worker.id,
                category: .overview,
                title: "项目背景",
                markdown: "普通成员不应写入",
                expectedRevision: nil,
                nowUnixMs: 200
            )
            XCTFail("A non-manager wrote a team asset")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }

        let firstRevision = try await store.upsertTeamAsset(
            ownerUserID: "alice",
            teamRoomID: room.id,
            assetID: nil,
            editorAgentID: manager.id,
            category: .techStack,
            title: "技术栈",
            markdown: "Swift 6 + SQLite",
            expectedRevision: nil,
            nowUnixMs: 201
        )
        do {
            _ = try await store.upsertTeamAsset(
                ownerUserID: "alice",
                teamRoomID: room.id,
                assetID: firstRevision.id,
                editorAgentID: manager.id,
                category: .techStack,
                title: "技术栈",
                markdown: "错误覆盖",
                expectedRevision: 0,
                nowUnixMs: 202
            )
            XCTFail("A stale asset revision was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }

        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "asset-snapshot-todo",
            draft: .init(
                title: "实现客户端",
                teamRoomID: room.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 203
        )
        let executorDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            nowUnixMs: 204
        )
        _ = try XCTUnwrap(executorDelivery)
        let updatedAsset = try await store.upsertTeamAsset(
            ownerUserID: "alice",
            teamRoomID: room.id,
            assetID: firstRevision.id,
            editorAgentID: manager.id,
            category: .techStack,
            title: "技术栈",
            markdown: "Swift 6 + PostgreSQL",
            expectedRevision: firstRevision.revision,
            nowUnixMs: 205
        )
        XCTAssertEqual(updatedAsset.revision, 2)

        let snapshots = try await store.listTodoTeamAssetSnapshots(
            ownerUserID: "alice",
            todoID: todo.id
        )
        XCTAssertEqual(snapshots.count, 1)
        XCTAssertEqual(snapshots[0].assetID, firstRevision.id)
        XCTAssertEqual(snapshots[0].revision, 1)
        XCTAssertEqual(snapshots[0].markdown, "Swift 6 + SQLite")
        let revisionOneSnapshot = try await store.todoTeamAssetSnapshot(
            ownerUserID: "alice",
            todoID: todo.id,
            assetID: firstRevision.id,
            revision: 1
        )
        XCTAssertNotNil(revisionOneSnapshot)
        let revisionTwoSnapshot = try await store.todoTeamAssetSnapshot(
            ownerUserID: "alice",
            todoID: todo.id,
            assetID: firstRevision.id,
            revision: 2
        )
        XCTAssertNil(revisionTwoSnapshot)
        let revisions = try await store.listTeamAssetRevisions(
            ownerUserID: "alice",
            teamRoomID: room.id,
            assetID: firstRevision.id,
            limit: 10
        )
        XCTAssertEqual(revisions.map(\.revision), [2, 1])
    }

    func testManagedTeamRequiresExplicitProjectManagerProfession() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let ordinary = try await makeAgent(store, name: "普通成员")
        do {
            _ = try await store.createManagedRoom(
                ownerUserID: "alice",
                projectID: "invalid-managed-team",
                draft: .init(name: "无项目经理团队"),
                projectManagerAgentID: ordinary.id
            )
            XCTFail("A non-project-manager profession created a managed team")
        } catch {
            XCTAssertEqual(
                error as? AgentGroupChatError,
                .invalidField("projectManagerProfession")
            )
        }
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护任务板。",
                modelConfigID: "model",
                professionKey: "project_manager"
            )
        )
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "managed-team",
            draft: .init(name: "有项目经理团队"),
            projectManagerAgentID: manager.id
        )
        XCTAssertEqual(room.projectManagerAgentID, manager.id)
        XCTAssertEqual(room.defaultAgentID, manager.id)
        let members = try await store.listMembers(ownerUserID: "alice", roomID: room.id)
        XCTAssertEqual(members.map(\.agentID), [manager.id])
    }

    func testMigration17RebuildsRoomTableWithCompositeManagerForeignKey() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        var initialStore: SQLiteAgentGroupChatStore? = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await initialStore!.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "管理任务。",
                modelConfigID: "model",
                professionKey: "project_manager"
            )
        )
        let room = try await initialStore!.createManagedRoom(
            ownerUserID: "alice",
            projectID: "migration-17-project",
            draft: .init(name: "迁移测试团队"),
            projectManagerAgentID: manager.id
        )
        initialStore = nil

        try executeSQLite(
            url,
            sql: """
            PRAGMA foreign_keys = OFF;
            BEGIN IMMEDIATE;
            CREATE TABLE project_agent_rooms_v16 (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                project_id TEXT NOT NULL,
                name TEXT NOT NULL,
                goal TEXT NOT NULL,
                default_agent_id TEXT,
                status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                conversation_kind TEXT NOT NULL DEFAULT 'project_team',
                direct_key TEXT,
                PRIMARY KEY(owner_user_id, id),
                FOREIGN KEY(owner_user_id, default_agent_id)
                    REFERENCES local_agent_profiles(owner_user_id, id)
            );
            INSERT INTO project_agent_rooms_v16
            SELECT owner_user_id, id, project_id, name, goal, default_agent_id,
                   status, created_at_unix_ms, updated_at_unix_ms,
                   conversation_kind, direct_key
            FROM project_agent_rooms;
            DROP TABLE project_agent_rooms;
            ALTER TABLE project_agent_rooms_v16 RENAME TO project_agent_rooms;
            CREATE UNIQUE INDEX one_active_agent_room_per_project
                ON project_agent_rooms(owner_user_id, project_id) WHERE status = 'active';
            CREATE UNIQUE INDEX one_active_direct_conversation_per_pair
                ON project_agent_rooms(owner_user_id, direct_key)
                WHERE status = 'active' AND direct_key IS NOT NULL;
            DELETE FROM local_agent_group_chat_schema_migrations WHERE version = 17;
            COMMIT;
            PRAGMA foreign_keys = ON;
            """
        )

        let migratedStore = try SQLiteAgentGroupChatStore(databaseURL: url)
        let migratedRoom = try await migratedStore.room(
            ownerUserID: "alice",
            roomID: room.id
        )
        XCTAssertEqual(migratedRoom?.projectManagerAgentID, manager.id)
        try executeSQLite(url, sql: "PRAGMA foreign_key_check;")
    }

    func testMigration22PreservesTodoDeliveryAndBackfillsEventRecipientOnReplay() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        var initialStore: SQLiteAgentGroupChatStore? = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await initialStore!.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护迁移任务板。",
                modelConfigID: "model",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(initialStore!, name: "迁移执行者")
        let room = try await initialStore!.createManagedRoom(
            ownerUserID: "alice",
            projectID: "migration-22-project",
            draft: .init(name: "迁移事件团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await initialStore!.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: worker.id,
            draft: .init(role: "执行者")
        )
        let todo = try await initialStore!.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "migration-22-todo",
            draft: .init(
                title: "验证事件迁移",
                teamRoomID: room.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 100
        )
        let createdReadyDelivery = try await initialStore!.enqueueAgentTodoReady(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            nowUnixMs: 101
        )
        let readyDelivery = try XCTUnwrap(createdReadyDelivery)
        initialStore = nil

        try executeSQLite(
            url,
            sql: """
            PRAGMA foreign_keys = OFF;
            BEGIN IMMEDIATE;
            DROP TABLE local_agent_todo_event_recipients;
            DELETE FROM local_agent_group_chat_schema_migrations WHERE version = 22;
            COMMIT;
            PRAGMA foreign_keys = ON;
            """
        )

        let migratedStore = try SQLiteAgentGroupChatStore(databaseURL: url)
        let migratedTodo = try await migratedStore.agentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id
        )
        XCTAssertEqual(migratedTodo?.title, todo.title)
        let migratedDelivery = try await migratedStore.delivery(
            ownerUserID: "alice",
            deliveryID: readyDelivery.id
        )
        XCTAssertEqual(migratedDelivery?.id, readyDelivery.id)

        let replayed = try await migratedStore.enqueueAgentTodoReady(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            nowUnixMs: 102
        )
        XCTAssertEqual(replayed?.id, readyDelivery.id)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_todo_event_recipients"
        ), 1)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_group_chat_schema_migrations WHERE version = 22"
        ), 1)
        try executeSQLite(url, sql: "PRAGMA foreign_key_check;")
    }

    func testMigration21RepairsMissingColumnEvenWhenMarkerAlreadyExists() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        var initialStore: SQLiteAgentGroupChatStore? = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(initialStore!, name: "迁移修复 Agent")
        let room = try await initialStore!.createRoom(
            ownerUserID: "alice",
            projectID: "migration-21-repair-project",
            draft: .init(name: "迁移修复团队")
        )
        _ = try await initialStore!.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await initialStore!.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "migration-21-repair",
            draft: .init(title: "保留旧任务", teamRoomID: room.id),
            nowUnixMs: 100
        )
        initialStore = nil

        try executeSQLite(
            url,
            sql: """
            PRAGMA foreign_keys = OFF;
            ALTER TABLE local_agent_todos DROP COLUMN execution_contract_json;
            PRAGMA foreign_keys = ON;
            """
        )
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_group_chat_schema_migrations WHERE version = 21"
        ), 1)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM pragma_table_info('local_agent_todos') WHERE name = 'execution_contract_json'"
        ), 0)

        let repairedStore = try SQLiteAgentGroupChatStore(databaseURL: url)
        let repairedTodo = try await repairedStore.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(repairedTodo?.title, todo.title)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM pragma_table_info('local_agent_todos') WHERE name = 'execution_contract_json'"
        ), 1)
    }

    func testProjectDashboardRequiresManagerAndUsesOptimisticRevisioning() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        XCTAssertEqual(
            try sqliteInt(
                url,
                sql: "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'local_agent_project_dashboards'"
            ),
            1
        )

        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护项目总览。",
                modelConfigID: "model-1",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(store, name: "执行者")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "dashboard-project",
            draft: .init(name: "总览团队"),
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
            requestKey: "dashboard-todo",
            draft: .init(
                title: "完成第一阶段",
                teamRoomID: room.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 100
        )
        let firstUpdate = LocalAgentProjectDashboardUpdate(
            phase: "实现阶段",
            health: .onTrack,
            summary: "核心功能正在实现。",
            nextSteps: ["完成联调"],
            milestones: [
                .init(
                    id: "milestone-1",
                    title: "第一阶段",
                    status: .inProgress,
                    progressPercent: 60,
                    linkedTodoIDs: [todo.id]
                ),
            ],
            issues: [
                .init(
                    id: "issue-1",
                    title: "等待确认",
                    requestedAction: "确认验收范围",
                    severity: .warning,
                    owner: .human,
                    relatedTodoID: todo.id
                ),
            ]
        )

        let created = try await store.upsertProjectDashboard(
            ownerUserID: "alice",
            teamRoomID: room.id,
            editorAgentID: manager.id,
            expectedRevision: nil,
            update: firstUpdate,
            nowUnixMs: 101
        )
        XCTAssertEqual(created.revision, 1)
        XCTAssertEqual(created.milestones.first?.linkedTodoIDs, [todo.id])
        let loaded = try await store.projectDashboard(
            ownerUserID: "alice",
            teamRoomID: room.id
        )
        XCTAssertEqual(loaded, created)

        let secondUpdate = LocalAgentProjectDashboardUpdate(
            phase: "联调阶段",
            health: .atRisk,
            summary: "进入联调，存在一项待确认事项。",
            nextSteps: ["完成联调", "确认验收范围"],
            milestones: firstUpdate.milestones,
            issues: firstUpdate.issues
        )
        let updated = try await store.upsertProjectDashboard(
            ownerUserID: "alice",
            teamRoomID: room.id,
            editorAgentID: manager.id,
            expectedRevision: 1,
            update: secondUpdate,
            nowUnixMs: 102
        )
        XCTAssertEqual(updated.revision, 2)
        XCTAssertEqual(updated.phase, "联调阶段")

        do {
            _ = try await store.upsertProjectDashboard(
                ownerUserID: "alice",
                teamRoomID: room.id,
                editorAgentID: manager.id,
                expectedRevision: 1,
                update: secondUpdate,
                nowUnixMs: 103
            )
            XCTFail("A stale dashboard revision was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }

        do {
            _ = try await store.upsertProjectDashboard(
                ownerUserID: "alice",
                teamRoomID: room.id,
                editorAgentID: worker.id,
                expectedRevision: 2,
                update: secondUpdate,
                nowUnixMs: 104
            )
            XCTFail("A non-manager updated the dashboard")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }

        let otherRoom = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "other-dashboard-project",
            draft: .init(name: "其他团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: otherRoom.id,
            agentID: worker.id,
            draft: .init(role: "执行者")
        )
        let otherTodo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "other-dashboard-todo",
            draft: .init(
                title: "其他团队任务",
                teamRoomID: otherRoom.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 105
        )
        let invalidUpdate = LocalAgentProjectDashboardUpdate(
            phase: "联调阶段",
            health: .atRisk,
            summary: "不应允许跨团队任务引用。",
            milestones: [
                .init(
                    id: "milestone-2",
                    title: "错误引用",
                    status: .pending,
                    progressPercent: 0,
                    linkedTodoIDs: [otherTodo.id]
                ),
            ]
        )
        do {
            _ = try await store.upsertProjectDashboard(
                ownerUserID: "alice",
                teamRoomID: room.id,
                editorAgentID: manager.id,
                expectedRevision: 2,
                update: invalidUpdate,
                nowUnixMs: 106
            )
            XCTFail("A dashboard linked a Todo from another team")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .invalidField("dashboardTodoRefs"))
        }
        let unchanged = try await store.projectDashboard(
            ownerUserID: "alice",
            teamRoomID: room.id
        )
        XCTAssertEqual(unchanged?.revision, 2)
    }

    func testTodoCompletionProgressPersistsAssetSuggestionsAndStatusCallsThemOut() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护任务与资产。",
                modelConfigID: "model-1",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(store, name: "执行者")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "suggestion-project",
            draft: .init(name: "建议团队"),
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
            requestKey: "suggestion-todo",
            draft: .init(
                title: "完成架构验证",
                teamRoomID: room.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 200
        )
        let suggestion = LocalAgentTeamAssetUpdateSuggestion(
            category: .architecture,
            title: "架构决策",
            markdown: "采用事件驱动更新。",
            rationale: "执行验证已确认该方案通过验收。"
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            update: .init(status: .inProgress),
            nowUnixMs: 200
        )
        _ = try await store.appendAgentTodoProgress(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            kind: .completed,
            runID: "run-suggestion",
            stage: "completed",
            detail: "验证完成",
            assetUpdateSuggestions: [suggestion],
            nowUnixMs: 201
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            update: .init(status: .completed, result: "验证完成"),
            nowUnixMs: 202
        )
        let progress = try await store.listAgentTodoProgress(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            limit: 10
        )
        XCTAssertEqual(progress.last?.assetUpdateSuggestions, [suggestion])
        let deliveries = try await store.enqueueAgentTodoStatus(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            excludingAgentID: worker.id,
            nowUnixMs: 203
        )
        let managerStatus = try XCTUnwrap(deliveries.first)
        let message = try await store.message(
            ownerUserID: "alice",
            roomID: managerStatus.roomID,
            messageID: managerStatus.messageID
        )
        XCTAssertTrue(try XCTUnwrap(message).content.contains("共享资产更新建议：1 条"))
    }
}
