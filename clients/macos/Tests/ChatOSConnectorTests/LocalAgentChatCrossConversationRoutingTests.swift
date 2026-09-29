import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class LocalAgentChatCrossConversationRoutingTests: XCTestCase {
    func testGlobalRunCanRecoverFromMentionTargetingAnotherConversation() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("cross-conversation-routing-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let service = NativeAgentGroupChatService(
            databaseURL: root.appendingPathComponent("chat.db")
        )
        let store = try await service.store()
        let reporter = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(name: "视觉", rolePrompt: "负责视觉。", modelConfigID: "model")
        )
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "负责项目管理。",
                modelConfigID: "model",
                professionKey: "project_manager"
            )
        )
        let team = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "project-1",
            draft: .init(name: "项目团队")
        )
        for (agent, role) in [(reporter, "视觉"), (manager, "项目经理")] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: team.id,
                agentID: agent.id,
                draft: .init(role: role)
            )
        }
        _ = try await store.setProjectManager(
            ownerUserID: "alice",
            roomID: team.id,
            agentID: manager.id
        )
        let direct = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: reporter.id
        )
        let trigger = try await store.postMessage(
            ownerUserID: "alice",
            roomID: direct.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "检查全部未读和任务。",
                mentionedAgentIDs: [reporter.id]
            ),
            limits: .init()
        ).message
        let claimedDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: reporter.id,
            nowUnixMs: trigger.createdAtUnixMs + 1
        )
        let delivery = try XCTUnwrap(claimedDelivery)
        let provider = try await LocalAgentRelayMCPServer(service: service).connect(
            context: try .init(
                ownerUserID: "alice",
                projectID: direct.projectID,
                roomID: direct.id,
                agentID: reporter.id,
                deliveryID: delivery.id,
                triggerMessageID: delivery.messageID,
                rootMessageID: delivery.rootMessageID,
                runID: "global-manager-run",
                hopCount: delivery.hopCount
            )
        )
        _ = try await provider.definitions()
        let workspace = try await provider.execute(.init(
            id: "workspace",
            name: LocalAgentChatToolProvider.workspaceSnapshotToolName,
            arguments: "{}"
        ))
        let workspaceJSON = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(workspace.content.utf8)) as? [String: Any]
        )
        let teamJSON = try XCTUnwrap(
            (workspaceJSON["teams"] as? [[String: Any]])?.first(where: {
                ($0["name"] as? String) == "项目团队"
            })
        )
        let teamReference = try XCTUnwrap(teamJSON["team_ref"] as? String)
        let managerReference = try XCTUnwrap(
            (teamJSON["members"] as? [[String: Any]])?.first(where: {
                ($0["name"] as? String) == "项目经理"
            })?["agent_ref"] as? String
        )
        let arguments = try toolArguments([
            "content": "任务完成，请项目经理检查。",
            "mention_agent_refs": [managerReference],
        ])

        let rejected = try await provider.execute(.init(
            id: "wrong-target",
            name: LocalAgentChatToolProvider.sendMessageToolName,
            arguments: arguments
        ))

        XCTAssertTrue(rejected.isError)
        XCTAssertTrue(rejected.content.contains(#""code":"mention_not_in_target_conversation""#))
        XCTAssertTrue(rejected.content.contains(#""next_tool":"chat_send_message""#))
        XCTAssertTrue(rejected.content.contains("target_agent_ref"))
        let directMessages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: direct.id,
            limit: 20
        )
        XCTAssertEqual(directMessages.count, 1)
        let persistedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(persistedDelivery?.status, .running)

        let routed = try await provider.execute(.init(
            id: "correct-target",
            name: LocalAgentChatToolProvider.sendMessageToolName,
            arguments: try toolArguments([
                "team_ref": teamReference,
                "content": "任务完成，请项目经理检查。",
                "mention_agent_refs": [managerReference],
            ])
        ))
        XCTAssertFalse(routed.isError, routed.content)
        let teamMessages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: team.id,
            limit: 20
        )
        XCTAssertEqual(teamMessages.last?.senderID, reporter.id)
        XCTAssertEqual(teamMessages.last?.mentionedAgentIDs, [manager.id])

        let directSend = try await provider.execute(.init(
            id: "one-step-direct",
            name: LocalAgentChatToolProvider.sendMessageToolName,
            arguments: try toolArguments([
                "target_agent_ref": managerReference,
                "content": "这是一条主动私聊。",
            ])
        ))
        XCTAssertFalse(directSend.isError, directSend.content)
        let directConversations = try await store.listDirectConversations(
            ownerUserID: "alice",
            includeArchived: false
        )
        let agentDirect = try XCTUnwrap(directConversations.first(where: {
            $0.conversationKind == .agentAgentDirect
        }))
        let directMembers = try await store.listMembers(
            ownerUserID: "alice",
            roomID: agentDirect.id
        )
        XCTAssertEqual(Set(directMembers.map(\.agentID)), Set([reporter.id, manager.id]))
        let proactiveMessages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: agentDirect.id,
            limit: 20
        )
        XCTAssertEqual(proactiveMessages.last?.content, "这是一条主动私聊。")
        let proactiveWake = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: manager.id,
            lane: .manager,
            nowUnixMs: (proactiveMessages.last?.createdAtUnixMs ?? 0) + 1
        )
        XCTAssertNotNil(proactiveWake)
    }

    private func toolArguments(_ value: [String: Any]) throws -> String {
        let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
        return try XCTUnwrap(String(data: data, encoding: .utf8))
    }
}
