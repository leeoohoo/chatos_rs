import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class LocalAgentChatToolProviderTests: XCTestCase {
    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("local-chat-tools-\(UUID().uuidString)")
            .appendingPathComponent("chat.db")
    }

    func testProviderReadsScopedContextAndCompletesDeliveryBySendingMessage() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let first = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "架构师",
                rolePrompt: "设计系统。",
                modelConfigID: "model",
                thinkingLevel: "medium",
                defaultSkillIDs: LocalAgentPermission.normalized(
                    preserving: [],
                    canManageStaff: true
                )
            )
        )
        let second = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(name: "客户端", rolePrompt: "实现客户端。", modelConfigID: "model")
        )
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "project-1",
            draft: .init(name: "项目群聊")
        )
        for (agent, role) in [(first, "架构师"), (second, "客户端")] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: role)
            )
        }
        let incoming = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "@架构师 请设计一下",
                mentionedAgentIDs: [first.id]
            ),
            limits: .init()
        )
        let firstClaim = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: first.id,
            nowUnixMs: incoming.message.createdAtUnixMs + 1
        )
        let claimed = try XCTUnwrap(firstClaim)
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: "project-1",
            roomID: room.id,
            agentID: first.id,
            deliveryID: claimed.id,
            triggerMessageID: incoming.message.id,
            rootMessageID: incoming.message.rootMessageID,
            runID: "run-1",
            hopCount: claimed.hopCount
        )
        let relayMCP = LocalAgentRelayMCPServer(
            service: NativeAgentGroupChatService(databaseURL: url),
            now: { incoming.message.createdAtUnixMs + 2 }
        )
        let provider = try await relayMCP.connect(context: context)

        let definitions = try await provider.definitions()
        XCTAssertEqual(
            Set(definitions.map(\.name)),
            [
                "relay_bootstrap", "chat_get_trigger", "chat_list_members", "chat_read_unread",
                "chat_read_messages", "chat_read_attachment", "chat_mark_read", "agent_propose_member",
                "agent_propose_member_removal",
                "chat_direct_open", "chat_direct_send", "chat_send_message",
            ]
        )
        let bootstrap = try await provider.execute(
            .init(id: "call-bootstrap", name: "relay_bootstrap", arguments: "{}")
        )
        XCTAssertFalse(bootstrap.content.contains("project-1"))
        XCTAssertTrue(bootstrap.content.contains(first.id))
        XCTAssertTrue(bootstrap.content.contains(second.id))
        XCTAssertTrue(bootstrap.content.contains(incoming.message.id))
        XCTAssertTrue(bootstrap.content.contains("unread"))
        let trigger = try await provider.execute(
            .init(id: "call-trigger", name: "chat_get_trigger", arguments: "{}")
        )
        XCTAssertTrue(trigger.content.contains(incoming.message.id))
        XCTAssertTrue(trigger.content.contains("请设计一下"))
        let members = try await provider.execute(
            .init(id: "call-members", name: "chat_list_members", arguments: "{}")
        )
        XCTAssertTrue(members.content.contains("架构师"))
        XCTAssertTrue(members.content.contains("客户端"))
        let unread = try await provider.execute(
            .init(id: "call-unread", name: "chat_read_unread", arguments: #"{"limit":20}"#)
        )
        XCTAssertTrue(unread.content.contains(incoming.message.id))
        let marked = try await provider.execute(
            .init(
                id: "call-mark-read",
                name: "chat_mark_read",
                arguments: #"{"through_message_id":"\#(incoming.message.id)"}"#
            )
        )
        XCTAssertTrue(marked.content.contains(#""has_unread":false"#))
        let proposalArguments = try XCTUnwrap(
            String(
                data: JSONSerialization.data(withJSONObject: [
                    "name": "测试 Agent",
                    "role": "测试工程师",
                    "responsibility": "验证实现",
                    "role_prompt": "只验证当前项目的实现。",
                    "model_config_id": "default",
                    "profession_key": "qa_engineer",
                    "rationale": "团队缺少测试角色",
                ], options: [.sortedKeys]),
                encoding: .utf8
            )
        )
        let proposed = try await provider.execute(
            .init(
                id: "call-propose-member",
                name: "agent_propose_member",
                arguments: proposalArguments
            )
        )
        XCTAssertTrue(proposed.content.contains("测试 Agent"))
        XCTAssertTrue(proposed.content.contains(#""status":"pending""#))
        let pendingProposals = try await store.listAgentProposals(
            ownerUserID: "alice",
            roomID: room.id,
            status: .pending
        )
        XCTAssertEqual(pendingProposals.count, 1)
        XCTAssertEqual(pendingProposals.first?.proposerAgentID, first.id)
        XCTAssertEqual(pendingProposals.first?.draft.modelConfigID, first.draft.modelConfigID)
        XCTAssertEqual(pendingProposals.first?.draft.thinkingLevel, "medium")

        let sendArguments = try XCTUnwrap(
            String(
                data: JSONSerialization.data(withJSONObject: [
                    "content": "方案完成，@客户端 请开始实现。",
                    "mention_agent_ids": [second.id],
                ], options: [.sortedKeys]),
                encoding: .utf8
            )
        )
        let sent = try await provider.execute(
            .init(
                id: "call-send",
                name: "chat_send_message",
                arguments: sendArguments
            )
        )
        XCTAssertTrue(sent.content.contains("spawned_delivery_ids"))
        let completed = try await store.delivery(ownerUserID: "alice", deliveryID: claimed.id)
        XCTAssertEqual(completed?.status, .completed)
        let next = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: second.id,
            nowUnixMs: incoming.message.createdAtUnixMs + 3
        )
        XCTAssertEqual(next?.triggerKind, .agentMention)
        XCTAssertEqual(next?.rootMessageID, incoming.message.rootMessageID)

        do {
            _ = try await provider.execute(
                .init(id: "call-send-again", name: "chat_send_message", arguments: sendArguments)
            )
            XCTFail("Completed delivery accepted a second response")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }
        let transcript = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 20
        )
        XCTAssertEqual(transcript.count, 2)
        XCTAssertEqual(transcript.last?.senderID, first.id)
        XCTAssertEqual(transcript.last?.sourceRunID, "run-1")
    }
}
