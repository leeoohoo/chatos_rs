@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
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

}
