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

}
