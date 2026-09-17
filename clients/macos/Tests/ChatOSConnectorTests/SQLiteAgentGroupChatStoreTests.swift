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
            modelConfigID: "model-1",
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

        let approval = try await store.approveAgentProposal(
            ownerUserID: "alice",
            roomID: room.id,
            proposalID: proposal.id,
            nowUnixMs: incoming.message.createdAtUnixMs + 4
        )
        XCTAssertEqual(approval.proposal.status, .approved)
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
        let agent = try await makeAgent(
            store,
            name: "团队负责人",
            canAccessLocalProjects: true
        )
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
            context: context,
            now: { post.message.createdAtUnixMs + 2 }
        )
        let definitions = try await provider.definitions()
        XCTAssertEqual(definitions.map(\.name), ["team_propose"])
        let schema = String(decoding: try XCTUnwrap(definitions.first).schema, as: UTF8.self)
        XCTAssertTrue(schema.contains("设计系统"))
        XCTAssertTrue(schema.contains("existing_1"))
        XCTAssertFalse(schema.contains(secretProjectID))

        let result = try await provider.execute(.init(
            id: "team-proposal-call",
            name: "team_propose",
            arguments: #"{"project_option":"existing_1","team_name":"设计团队","team_goal":"完成产品设计"}"#
        ))
        XCTAssertFalse(result.content.contains(secretProjectID))
        let proposals = try await store.listTeamProposals(
            ownerUserID: "alice",
            sourceRoomID: room.id,
            status: .pending
        )
        let proposal = try XCTUnwrap(proposals.first)
        XCTAssertEqual(proposal.draft.existingProjectID, secretProjectID)

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
}
