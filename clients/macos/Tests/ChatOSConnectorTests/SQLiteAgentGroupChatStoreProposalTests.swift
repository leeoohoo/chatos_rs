@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
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

}
