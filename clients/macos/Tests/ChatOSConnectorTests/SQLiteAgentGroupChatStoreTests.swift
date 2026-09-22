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
        let provider = LocalAgentProjectToolProvider(
            store: store,
            projects: [target],
            projectsService: projectsService,
            context: context,
            now: { post.message.createdAtUnixMs + 2 }
        )
        let definitions = try await provider.definitions()
        XCTAssertEqual(definitions.map(\.name), [
            "project_catalog",
            "team_propose_existing",
            "team_propose_new_project",
            "team_propose_import_directory",
        ])
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
        let listedForAgent = try await reopened.listAgentRuns(
            ownerUserID: "alice",
            agentID: agent.id,
            limit: 10
        )
        XCTAssertEqual(listedForAgent, [run])
        let listedForRoom = try await reopened.listRoomRuns(
            ownerUserID: "alice",
            roomID: room.id,
            limit: 10
        )
        XCTAssertEqual(listedForRoom, [run])
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

        let firstPendingValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
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

        let retriedPendingValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 105
        )
        let retriedPending = try XCTUnwrap(retriedPendingValue)
        XCTAssertEqual(retriedPending.id, firstPending.id)
        XCTAssertEqual(retriedPending.status, .pending)
        XCTAssertEqual(retriedPending.attempt, 1)
        XCTAssertNil(retriedPending.lastError)
        XCTAssertNil(retriedPending.claimedAtUnixMs)
        XCTAssertNil(retriedPending.completedAtUnixMs)
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
        let selected = try await store.todoForDelivery(
            ownerUserID: "alice",
            deliveryID: firstDelivery.id
        )
        XCTAssertEqual(selected?.id, ready.id)
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

    func testMigration30ReplacesEmptyTeamOwnedSurveyTableWithProjectOwnedSchema() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        var initialStore: SQLiteAgentGroupChatStore? = try SQLiteAgentGroupChatStore(databaseURL: url)
        XCTAssertNotNil(initialStore)
        initialStore = nil

        try executeSQLite(
            url,
            sql: """
            PRAGMA foreign_keys = OFF;
            BEGIN IMMEDIATE;
            DROP TABLE local_agent_requirement_surveys;
            CREATE TABLE local_agent_requirement_surveys (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                team_room_id TEXT NOT NULL,
                creator_agent_id TEXT NOT NULL,
                source_delivery_id TEXT NOT NULL,
                request_key TEXT NOT NULL,
                draft_json TEXT NOT NULL,
                status TEXT NOT NULL,
                submission_json TEXT,
                resolution_json TEXT,
                created_at_unix_ms INTEGER NOT NULL,
                submitted_at_unix_ms INTEGER,
                resolved_at_unix_ms INTEGER,
                PRIMARY KEY(owner_user_id, id)
            );
            CREATE INDEX local_agent_requirement_surveys_team
                ON local_agent_requirement_surveys(owner_user_id, team_room_id);
            DELETE FROM local_agent_group_chat_schema_migrations WHERE version = 30;
            COMMIT;
            PRAGMA foreign_keys = ON;
            """
        )

        let migratedStore = try SQLiteAgentGroupChatStore(databaseURL: url)
        _ = migratedStore
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM pragma_table_info('local_agent_requirement_surveys') WHERE name = 'project_id'"
        ), 1)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM pragma_table_info('local_agent_requirement_surveys') WHERE name = 'team_room_id'"
        ), 0)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_group_chat_schema_migrations WHERE version = 30"
        ), 1)
    }

    func testRequirementSurveyPersistsHumanAnswersWakesOwnerAndStoresResolution() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护团队目标和计划。",
                modelConfigID: "model-1",
                professionKey: "project_manager"
            )
        )
        let researcher = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "需求调研 Agent",
                rolePrompt: "按需求调研 Skill 工作。",
                modelConfigID: "model-1",
                professionKey: "business_analyst"
            )
        )
        let ordinary = try await makeAgent(store, name: "普通成员")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "survey-project",
            draft: .init(name: "需求调研团队"),
            projectManagerAgentID: manager.id
        )
        for agent in [researcher, ordinary] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: agent.draft.name)
            )
        }
        let claimedManagerDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: manager.id,
            nowUnixMs: 100
        )
        let managerDelivery = try XCTUnwrap(claimedManagerDelivery)
        let draft = LocalAgentRequirementSurveyDraft(
            title: "重大变更确认",
            purpose: "确认兼容策略和发布范围，答案将决定迁移方案。",
            questions: [
                .init(
                    id: "compatibility",
                    prompt: "旧接口需要保留多久？",
                    kind: .singleChoice,
                    options: [
                        .init(id: "one_release", label: "保留一个版本"),
                        .init(id: "two_releases", label: "保留两个版本"),
                    ]
                ),
                .init(
                    id: "release_targets",
                    prompt: "本次包含哪些客户端？",
                    kind: .multipleChoice,
                    options: [
                        .init(id: "macos", label: "macOS"),
                        .init(id: "windows", label: "Windows"),
                    ]
                ),
            ]
        )
        let survey = try await store.createRequirementSurvey(
            ownerUserID: "alice",
            projectID: room.projectID,
            creatorAgentID: manager.id,
            sourceDeliveryID: managerDelivery.id,
            requestKey: "major-change-v1",
            draft: draft,
            nowUnixMs: 101
        )
        XCTAssertEqual(survey.projectID, room.projectID)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM pragma_table_info('local_agent_requirement_surveys') WHERE name = 'project_id'"
        ), 1)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM pragma_table_info('local_agent_requirement_surveys') WHERE name = 'team_room_id'"
        ), 0)
        let duplicate = try await store.createRequirementSurvey(
            ownerUserID: "alice",
            projectID: room.projectID,
            creatorAgentID: manager.id,
            sourceDeliveryID: managerDelivery.id,
            requestKey: "major-change-v1",
            draft: draft,
            nowUnixMs: 102
        )
        XCTAssertEqual(survey.id, duplicate.id)
        let pendingSurveys = try await store.listRequirementSurveys(
            ownerUserID: "alice",
            projectID: room.projectID,
            status: .pending
        )
        XCTAssertEqual(pendingSurveys.count, 1)

        let taskRunnerSurvey = try await store.createRequirementSurvey(
            ownerUserID: "alice",
            projectID: room.projectID,
            creatorAgentID: "task-runner",
            sourceDeliveryID: "task-run-1",
            requestKey: "task-runner-survey",
            draft: draft,
            nowUnixMs: 103
        )
        XCTAssertEqual(taskRunnerSurvey.creatorAgentID, "task-runner")

        do {
            _ = try await store.submitRequirementSurvey(
                ownerUserID: "alice",
                projectID: room.projectID,
                surveyID: survey.id,
                submission: .init(answers: [], notes: ""),
                nowUnixMs: 104
            )
            XCTFail("Required questions were accepted without answers")
        } catch {
            XCTAssertEqual(
                error as? AgentGroupChatError,
                .invalidField("requirementSurveyAnswers")
            )
        }
        let submission = LocalAgentRequirementSurveySubmission(
            answers: [
                .init(questionID: "compatibility", selectedOptionIDs: ["two_releases"]),
                .init(questionID: "release_targets", selectedOptionIDs: ["macos", "windows"]),
            ],
            notes: "Windows 需要晚一周灰度。"
        )
        let submitted = try await store.submitRequirementSurvey(
            ownerUserID: "alice",
            projectID: room.projectID,
            surveyID: survey.id,
            submission: submission,
            nowUnixMs: 105
        )
        let repeatedSubmission = try await store.submitRequirementSurvey(
            ownerUserID: "alice",
            projectID: room.projectID,
            surveyID: survey.id,
            submission: submission,
            nowUnixMs: 106
        )
        XCTAssertEqual(submitted, repeatedSubmission)
        XCTAssertEqual(submitted.submission?.notes, "Windows 需要晚一周灰度。")
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM project_agent_deliveries WHERE deduplication_key = 'requirement-survey-submitted:\(survey.id)'"
        ), 1)

        let resolution = LocalAgentRequirementSurveyResolution(
            summary: "旧接口保留两个版本，macOS 与 Windows 分阶段发布。",
            solutionMarkdown: "## 方案\n先兼容双版本，再逐步切换默认接口。",
            executionSteps: [
                .init(
                    id: "compat-layer",
                    title: "实现兼容层",
                    detail: "同时支持新旧接口并记录旧接口调用。",
                    owner: "后端工程师",
                    deliverable: "兼容层与调用指标",
                    acceptanceCriteria: "新旧客户端均通过回归测试"
                ),
            ],
            risksAndOpenQuestions: "Windows 灰度窗口需要单独观察。",
            relatedMaterials: "项目共享资产：接口兼容规范。"
        )
        let resolved = try await store.resolveRequirementSurvey(
            ownerUserID: "alice",
            projectID: room.projectID,
            surveyID: survey.id,
            resolverAgentID: researcher.id,
            resolution: resolution,
            nowUnixMs: 107
        )
        XCTAssertEqual(resolved.resolution, resolution)
        XCTAssertEqual(resolved.resolvedAtUnixMs, 107)
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
