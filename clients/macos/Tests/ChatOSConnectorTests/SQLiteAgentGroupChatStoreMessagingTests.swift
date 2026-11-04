@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
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

    func testArchiveRoomHidesTeamStopsQueuedWorkAndAllowsReplacement() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "待删除团队 Agent")
        let room = try await makeRoom(store, projectID: "removed-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        _ = try await store.setDefaultAgent(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id
        )
        let posted = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "稍后执行"),
            limits: .init()
        )
        let delivery = try XCTUnwrap(posted.deliveries.first)

        let archived = try await store.archiveRoom(
            ownerUserID: "alice",
            roomID: room.id,
            nowUnixMs: posted.message.createdAtUnixMs + 1
        )
        let activeRooms = try await store.listRooms(ownerUserID: "alice")
        let allRooms = try await store.listRooms(ownerUserID: "alice", includeArchived: true)
        let stoppedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        let activeMembers = try await store.listActiveMembers(ownerUserID: "alice")

        XCTAssertEqual(archived.status, .archived)
        XCTAssertTrue(activeRooms.isEmpty)
        XCTAssertEqual(allRooms.first?.id, room.id)
        XCTAssertEqual(stoppedDelivery?.status, .cancelled)
        XCTAssertFalse(activeMembers.contains { $0.roomID == room.id })
        let replacement = try await makeRoom(store, projectID: "removed-project")
        XCTAssertNotEqual(replacement.id, room.id)
    }

    func testArchiveAgentRemovesActiveStateButPreservesHistory() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let removed = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "待删除 Agent",
                rolePrompt: "处理任务。",
                modelConfigID: "model-1",
                heartbeatEnabled: true
            )
        )
        let replacement = try await makeAgent(store, name: "保留 Agent")
        let room = try await makeRoom(store, projectID: "agent-archive-project")
        _ = try await store.addMember(
            ownerUserID: "alice", roomID: room.id, agentID: removed.id,
            draft: .init(role: "负责人")
        )
        _ = try await store.addMember(
            ownerUserID: "alice", roomID: room.id, agentID: replacement.id,
            draft: .init(role: "接替者")
        )
        _ = try await store.setDefaultAgent(
            ownerUserID: "alice", roomID: room.id, agentID: removed.id
        )
        let direct = try await store.openHumanAgentDirect(
            ownerUserID: "alice", agentID: removed.id
        )
        let directPost = try await store.postMessage(
            ownerUserID: "alice",
            roomID: direct.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "保留这条历史消息"),
            limits: .init()
        )
        let delivery = try XCTUnwrap(directPost.deliveries.first)
        let claimedDeliveryValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: removed.id,
            nowUnixMs: directPost.message.createdAtUnixMs + 1
        )
        let claimedDelivery = try XCTUnwrap(claimedDeliveryValue)
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: direct.projectID,
            roomID: direct.id,
            agentID: removed.id,
            deliveryID: claimedDelivery.id,
            triggerMessageID: directPost.message.id,
            rootMessageID: directPost.message.rootMessageID,
            runID: runID.uuidString.lowercased(),
            hopCount: claimedDelivery.hopCount
        )
        var checkpoint = AgentRunCheckpoint(
            scope: LocalAgentGroupChatRun.runtimeScope(for: context),
            messages: [.init(role: .system, content: "system")]
        )
        checkpoint.id = runID
        checkpoint.status = .paused
        try await store.saveRun(try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: removed.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: directPost.message.createdAtUnixMs + 1,
            updatedAtUnixMs: directPost.message.createdAtUnixMs + 1
        ))
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: removed.id,
            requestKey: "archive-agent-todo",
            draft: .init(title: "未完成任务", teamRoomID: room.id),
            nowUnixMs: directPost.message.createdAtUnixMs
        )

        let archived = try await store.archiveAgent(
            ownerUserID: "alice",
            agentID: removed.id,
            nowUnixMs: directPost.message.createdAtUnixMs + 1
        )

        let activeAgents = try await store.listAgents(ownerUserID: "alice")
        let allAgents = try await store.listAgents(ownerUserID: "alice", includeArchived: true)
        let loadedRoom = try await store.room(ownerUserID: "alice", roomID: room.id)
        let updatedRoom = try XCTUnwrap(loadedRoom)
        let members = try await store.listMembers(ownerUserID: "alice", roomID: room.id)
        let stoppedDelivery = try await store.delivery(
            ownerUserID: "alice", deliveryID: delivery.id
        )
        let stoppedRun = try await store.run(ownerUserID: "alice", runID: runID)
        let cancelledTodo = try await store.agentTodo(
            ownerUserID: "alice", agentID: removed.id, todoID: todo.id
        )
        let activeDirects = try await store.listDirectConversations(ownerUserID: "alice")
        let allDirects = try await store.listDirectConversations(
            ownerUserID: "alice", includeArchived: true
        )
        let historicalMessages = try await store.listMessages(
            ownerUserID: "alice", roomID: direct.id, afterUnixMs: nil, limit: 20
        )

        XCTAssertEqual(archived.status, .archived)
        XCTAssertFalse(archived.draft.heartbeatEnabled)
        XCTAssertNil(archived.nextHeartbeatAtUnixMs)
        XCTAssertEqual(activeAgents.map(\.id), [replacement.id])
        XCTAssertTrue(allAgents.contains { $0.id == removed.id && $0.status == .archived })
        XCTAssertEqual(updatedRoom.defaultAgentID, replacement.id)
        XCTAssertEqual(members.map(\.agentID), [replacement.id])
        XCTAssertEqual(stoppedDelivery?.status, .failed)
        XCTAssertEqual(stoppedRun?.checkpoint.status, .failed)
        XCTAssertEqual(stoppedRun?.checkpoint.stopReason, "Agent 已由用户删除。")
        XCTAssertEqual(cancelledTodo?.status, .cancelled)
        XCTAssertTrue(activeDirects.isEmpty)
        XCTAssertEqual(allDirects.first(where: { $0.id == direct.id })?.status, .archived)
        XCTAssertTrue(historicalMessages.contains { $0.id == directPost.message.id })
    }

    func testArchiveAgentIsOwnerScoped() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, owner: "alice", name: "Alice Agent")

        do {
            _ = try await store.archiveAgent(
                ownerUserID: "bob", agentID: agent.id, nowUnixMs: 10
            )
            XCTFail("another owner must not archive the Agent")
        } catch let error as AgentGroupChatError {
            guard case .notFound = error else { return XCTFail("unexpected error: \(error)") }
        }

        let aliceAgentIDs = try await store.listAgents(ownerUserID: "alice").map(\.id)
        XCTAssertEqual(aliceAgentIDs, [agent.id])
    }

    func testArchiveProjectManagerTransfersAuthorityOnlyToAnotherManager() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        func makeManager(_ name: String) async throws -> LocalAgentProfile {
            try await store.createAgent(
                ownerUserID: "alice",
                draft: .init(
                    name: name,
                    rolePrompt: "管理项目。",
                    modelConfigID: "model-1",
                    professionKey: "project_manager"
                )
            )
        }
        let removedManager = try await makeManager("原项目经理")
        let replacementManager = try await makeManager("接任项目经理")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "manager-transfer-project",
            draft: .init(name: "经理交接团队"),
            projectManagerAgentID: removedManager.id
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: replacementManager.id,
            draft: .init(role: "候补项目经理")
        )

        _ = try await store.archiveAgent(
            ownerUserID: "alice",
            agentID: removedManager.id,
            nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
        )

        let activeRooms = try await store.listRooms(ownerUserID: "alice")
        let loadedRoom = try XCTUnwrap(activeRooms.first)
        XCTAssertEqual(loadedRoom.defaultAgentID, replacementManager.id)
        XCTAssertEqual(loadedRoom.projectManagerAgentID, replacementManager.id)
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

#if DEBUG
        let before = await store.preparedStatementCountForTesting()
#endif
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
#if DEBUG
        let statementCount = await store.preparedStatementCountForTesting() - before
#endif

#if DEBUG
        XCTAssertEqual(statementCount, 8)
#endif
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

}
