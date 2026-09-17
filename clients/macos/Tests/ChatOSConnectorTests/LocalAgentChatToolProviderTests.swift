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
                professionKey: "project_manager",
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
        _ = try await store.setProjectManager(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: first.id
        )
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
                "relay_bootstrap", "agent_workspace_snapshot", "chat_get_trigger",
                "chat_list_members", "chat_read_unread",
                "chat_read_messages", "chat_read_attachment", "chat_mark_read", "agent_propose_member",
                "agent_propose_member_removal",
                "chat_direct_open", "chat_direct_send", "chat_send_message",
                "chat_read_all_unread", "chat_inbox_send",
                "todo_list", "todo_execution_options", "todo_add", "todo_update",
                "todo_reorder", "todo_read_progress", "todo_dependency_options",
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
        let workspace = try await provider.execute(
            .init(
                id: "call-workspace",
                name: LocalAgentChatToolProvider.workspaceSnapshotToolName,
                arguments: "{}"
            )
        )
        XCTAssertTrue(workspace.content.contains("项目群聊"))
        XCTAssertTrue(workspace.content.contains("架构师"))
        XCTAssertTrue(workspace.content.contains("客户端"))
        XCTAssertTrue(workspace.content.contains(#""has_project_manager":true"#))
        XCTAssertFalse(workspace.content.contains(room.id))
        XCTAssertFalse(workspace.content.contains(first.id))
        XCTAssertFalse(workspace.content.contains(second.id))
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

    func testHeartbeatCanCompleteQuietlyWithoutWritingTranscriptMessage() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let service = NativeAgentGroupChatService(databaseURL: url)
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "巡检员",
                rolePrompt: "检查进展。",
                modelConfigID: "model",
                heartbeatEnabled: true,
                heartbeatIntervalSeconds: 60
            )
        )
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "heartbeat-project",
            draft: .init(name: "巡检群")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "巡检员")
        )
        let dueAt = try XCTUnwrap(agent.nextHeartbeatAtUnixMs)
        let queued = try await store.enqueueDueAgentHeartbeats(
            ownerUserID: "alice",
            nowUnixMs: dueAt
        )
        let pending = try XCTUnwrap(queued.first)
        let claimedValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: dueAt + 1
        )
        let claimed = try XCTUnwrap(claimedValue)
        let loadedHeartbeatRoom = try await store.room(
            ownerUserID: "alice",
            roomID: claimed.roomID
        )
        let heartbeatRoom = try XCTUnwrap(loadedHeartbeatRoom)
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: heartbeatRoom.projectID,
            roomID: heartbeatRoom.id,
            agentID: agent.id,
            deliveryID: claimed.id,
            triggerMessageID: claimed.messageID,
            rootMessageID: claimed.rootMessageID,
            runID: "heartbeat-run",
            hopCount: 0
        )
        let provider = try await LocalAgentRelayMCPServer(
            service: service,
            now: { dueAt + 2 }
        ).connect(context: context)

        let definitions = try await provider.definitions()
        XCTAssertTrue(definitions.contains {
            $0.name == LocalAgentChatToolProvider.completeHeartbeatToolName
        })
        _ = try await provider.execute(
            .init(
                id: "quiet-complete",
                name: LocalAgentChatToolProvider.completeHeartbeatToolName,
                arguments: "{}"
            )
        )
        let completed = try await store.delivery(ownerUserID: "alice", deliveryID: pending.id)
        XCTAssertEqual(completed?.status, .completed)
        XCTAssertNil(completed?.responseMessageID)
        let transcript = try await store.listMessages(
            ownerUserID: "alice",
            roomID: heartbeatRoom.id,
            limit: 20
        )
        XCTAssertTrue(transcript.isEmpty)
    }

    func testGlobalUnreadMarksReadAndOnlyActionableMessagesBecomeTodos() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let service = NativeAgentGroupChatService(databaseURL: url)
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "管理任务。",
                modelConfigID: "model",
                professionKey: "project_manager"
            )
        )
        let first = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "one",
            draft: .init(name: "项目一")
        )
        let second = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "two",
            draft: .init(name: "项目二")
        )
        for room in [first, second] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "成员")
            )
            _ = try await store.setProjectManager(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id
            )
        }
        let informational = try await store.postMessage(
            ownerUserID: "alice",
            roomID: first.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "今天放假通知"),
            limits: .init()
        )
        let actionable = try await store.postMessage(
            ownerUserID: "alice",
            roomID: second.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "请修复登录错误"),
            limits: .init()
        )
        let direct = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)
        let anchor = try await store.postMessage(
            ownerUserID: "alice",
            roomID: direct.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "检查收件箱"),
            limits: .init()
        )
        let claimedValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: anchor.message.createdAtUnixMs + 1
        )
        let claimed = try XCTUnwrap(claimedValue)
        let provider = try await LocalAgentRelayMCPServer(
            service: service,
            now: { anchor.message.createdAtUnixMs + 2 }
        ).connect(context: try .init(
            ownerUserID: "alice",
            projectID: direct.projectID,
            roomID: direct.id,
            agentID: agent.id,
            deliveryID: claimed.id,
            triggerMessageID: claimed.messageID,
            rootMessageID: claimed.rootMessageID,
            runID: "inbox-run",
            hopCount: 0
        ), todoPluginOptions: [
            .init(
                pluginID: "internal-plugin-secret",
                displayName: "本地测试插件",
                description: "只暴露安全目录信息"
            ),
        ])

        let inbox = try await provider.execute(.init(
            id: "read-all",
            name: LocalAgentChatToolProvider.readAllUnreadToolName,
            arguments: #"{"limit":20}"#
        ))
        XCTAssertTrue(inbox.content.contains("今天放假通知"))
        XCTAssertTrue(inbox.content.contains("请修复登录错误"))
        XCTAssertTrue(inbox.content.contains(#""marked_read":true"#))
        XCTAssertFalse(inbox.content.contains(first.id))
        XCTAssertFalse(inbox.content.contains(second.id))
        XCTAssertFalse(inbox.content.contains(informational.message.id))
        XCTAssertFalse(inbox.content.contains(actionable.message.id))
        let firstUnread = try await store.listUnreadMessages(
            ownerUserID: "alice", roomID: first.id, agentID: agent.id, limit: 20
        )
        let secondUnread = try await store.listUnreadMessages(
            ownerUserID: "alice", roomID: second.id, agentID: agent.id, limit: 20
        )
        XCTAssertTrue(firstUnread.messages.isEmpty)
        XCTAssertTrue(secondUnread.messages.isEmpty)

        let json = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(inbox.content.utf8)) as? [String: Any]
        )
        let conversations = try XCTUnwrap(json["conversations"] as? [[String: Any]])
        let actionableConversation = try XCTUnwrap(conversations.first(where: { conversation in
            (conversation["messages"] as? [[String: Any]])?.contains(where: {
                ($0["content"] as? String) == "请修复登录错误"
            }) == true
        }))
        let actionableMessage = try XCTUnwrap(
            (actionableConversation["messages"] as? [[String: Any]])?.first(where: {
                ($0["content"] as? String) == "请修复登录错误"
            })
        )
        let actionableMessageRef = try XCTUnwrap(actionableMessage["message_ref"] as? String)
        let actionableConversationRef = try XCTUnwrap(
            actionableConversation["conversation_ref"] as? String
        )
        let informationalConversation = try XCTUnwrap(conversations.first(where: { conversation in
            (conversation["messages"] as? [[String: Any]])?.contains(where: {
                ($0["content"] as? String) == "今天放假通知"
            }) == true
        }))
        let informationalMessage = try XCTUnwrap(
            (informationalConversation["messages"] as? [[String: Any]])?.first(where: {
                ($0["content"] as? String) == "今天放假通知"
            })
        )
        let informationalMessageRef = try XCTUnwrap(
            informationalMessage["message_ref"] as? String
        )
        let replyArguments = try XCTUnwrap(String(
            data: JSONSerialization.data(withJSONObject: [
                "conversation_ref": actionableConversationRef,
                "reply_to_message_ref": actionableMessageRef,
                "content": "收到，我会处理。",
            ], options: [.sortedKeys]),
            encoding: .utf8
        ))
        let reply = try await provider.execute(.init(
            id: "reply-from-inbox",
            name: LocalAgentChatToolProvider.inboxSendToolName,
            arguments: replyArguments
        ))
        XCTAssertTrue(reply.content.contains(#""sent":true"#))
        XCTAssertFalse(reply.content.contains(second.id))
        let options = try await provider.execute(.init(
            id: "todo-options",
            name: LocalAgentChatToolProvider.todoExecutionOptionsToolName,
            arguments: "{}"
        ))
        let optionsJSON = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(options.content.utf8)) as? [String: Any]
        )
        XCTAssertFalse(options.content.contains("internal-plugin-secret"))
        let teamRef = try XCTUnwrap(
            (optionsJSON["teams"] as? [[String: Any]])?.first(where: {
                ($0["name"] as? String) == second.draft.name
            })?["team_ref"] as? String
        )
        let pluginRef = try XCTUnwrap(
            (optionsJSON["plugins"] as? [[String: Any]])?.first?["plugin_ref"] as? String
        )
        let assigneeRef = try XCTUnwrap(
            ((optionsJSON["teams"] as? [[String: Any]])?.first(where: {
                ($0["name"] as? String) == second.draft.name
            })?["assignees"] as? [[String: Any]])?.first?["assignee_ref"] as? String
        )
        let invalidTeam = try await provider.execute(.init(
            id: "todo-invalid-team",
            name: LocalAgentChatToolProvider.todoAddToolName,
            arguments: #"{"title":"无效任务","team_ref":"team_forged","source_message_refs":["\#(actionableMessageRef)"],"requires_execution":true,"builtin_capabilities":[]}"#
        ))
        XCTAssertTrue(invalidTeam.isError)
        XCTAssertTrue(invalidTeam.content.contains(#""code":"invalid_team_ref""#))
        XCTAssertTrue(invalidTeam.content.contains(#""next_tool":"todo_execution_options""#))
        XCTAssertFalse(invalidTeam.content.contains(second.id))
        let invalidMessage = try await provider.execute(.init(
            id: "todo-invalid-message",
            name: LocalAgentChatToolProvider.todoAddToolName,
            arguments: #"{"title":"无效任务","team_ref":"\#(teamRef)","source_message_refs":["message_forged"],"requires_execution":true,"builtin_capabilities":[]}"#
        ))
        XCTAssertTrue(invalidMessage.isError)
        XCTAssertTrue(invalidMessage.content.contains(#""code":"invalid_source_message_ref""#))
        XCTAssertTrue(invalidMessage.content.contains(#""next_tool":"chat_read_all_unread""#))
        let todoArguments = try XCTUnwrap(String(
            data: JSONSerialization.data(withJSONObject: [
                "title": "修复登录错误",
                "priority": 90,
                "team_ref": teamRef,
                "assignee_ref": assigneeRef,
                "source_message_refs": [actionableMessageRef, informationalMessageRef],
                "requires_execution": true,
                "builtin_capabilities": ["project_read", "project_write", "terminal"],
                "plugin_hints": [[
                    "plugin_ref": pluginRef,
                    "reason": "验证 Todo 创建时可信选择 Plugin",
                ]],
            ], options: [.sortedKeys]),
            encoding: .utf8
        ))
        let created = try await provider.execute(.init(
            id: "todo-actionable",
            name: LocalAgentChatToolProvider.todoAddToolName,
            arguments: todoArguments
        ))
        XCTAssertFalse(created.isError)
        XCTAssertFalse(created.content.contains(second.id))
        XCTAssertFalse(created.content.contains(actionable.message.id))
        XCTAssertFalse(created.content.contains(informational.message.id))
        XCTAssertFalse(created.content.contains("internal-plugin-secret"))
        let todos = try await store.listAgentTodos(
            ownerUserID: "alice",
            agentID: agent.id,
            includeTerminal: false
        )
        XCTAssertEqual(todos.count, 1)
        XCTAssertEqual(todos.first?.title, "修复登录错误")
        XCTAssertEqual(todos.first?.sourceMessageID, actionable.message.id)
        XCTAssertEqual(todos.first?.teamRoomID, second.id)
        XCTAssertEqual(
            todos.first?.executionPlan.builtinCapabilities,
            [.projectRead, .projectWrite, .terminal]
        )
        XCTAssertEqual(todos.first?.executionPlan.plugins.map(\.pluginID), ["internal-plugin-secret"])
        let sources = try await store.listAgentTodoSources(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: try XCTUnwrap(todos.first?.id)
        )
        XCTAssertEqual(Set(sources.map(\.messageID)), Set([
            actionable.message.id,
            informational.message.id,
        ]))
    }

    func testOnlyExplicitProjectManagerCanBuildCrossAgentTeamTodoGraph() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let service = NativeAgentGroupChatService(databaseURL: url)
        let store = try await service.store()
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护团队任务板。",
                modelConfigID: "model",
                professionKey: "project_manager"
            )
        )
        let engineer = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "工程师",
                rolePrompt: "完成分配的任务。",
                modelConfigID: "model"
            )
        )
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "team-board-project",
            draft: .init(name: "团队任务板")
        )
        for agent in [manager, engineer] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: agent.draft.name)
            )
        }
        _ = try await store.setProjectManager(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: manager.id
        )
        let incoming = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "先做接口，再做客户端",
                mentionedAgentIDs: [manager.id]
            ),
            limits: .init()
        )
        let claimedValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: manager.id,
            nowUnixMs: incoming.message.createdAtUnixMs + 1
        )
        let claimed = try XCTUnwrap(claimedValue)
        let provider = try await LocalAgentRelayMCPServer(
            service: service,
            now: { incoming.message.createdAtUnixMs + 2 }
        ).connect(context: try .init(
            ownerUserID: "alice",
            projectID: room.projectID,
            roomID: room.id,
            agentID: manager.id,
            deliveryID: claimed.id,
            triggerMessageID: claimed.messageID,
            rootMessageID: claimed.rootMessageID,
            runID: "manager-team-board-run",
            hopCount: 0
        ))
        let inbox = try await provider.execute(.init(
            id: "team-board-inbox",
            name: LocalAgentChatToolProvider.readAllUnreadToolName,
            arguments: "{}"
        ))
        let inboxJSON = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(inbox.content.utf8)) as? [String: Any]
        )
        let sourceRef = try XCTUnwrap(
            ((inboxJSON["conversations"] as? [[String: Any]])?.first?["messages"]
                as? [[String: Any]])?.first?["message_ref"] as? String
        )
        let options = try await provider.execute(.init(
            id: "team-board-options",
            name: LocalAgentChatToolProvider.todoExecutionOptionsToolName,
            arguments: "{}"
        ))
        let optionsJSON = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(options.content.utf8)) as? [String: Any]
        )
        let team = try XCTUnwrap((optionsJSON["teams"] as? [[String: Any]])?.first)
        let teamRef = try XCTUnwrap(team["team_ref"] as? String)
        let assignees = try XCTUnwrap(team["assignees"] as? [[String: Any]])
        let managerRef = try XCTUnwrap(
            assignees.first(where: { ($0["name"] as? String) == "项目经理" })?["assignee_ref"]
                as? String
        )
        let engineerRef = try XCTUnwrap(
            assignees.first(where: { ($0["name"] as? String) == "工程师" })?["assignee_ref"]
                as? String
        )
        let prerequisite = try await provider.execute(.init(
            id: "create-prerequisite",
            name: LocalAgentChatToolProvider.todoAddToolName,
            arguments: #"{"title":"实现接口","team_ref":"\#(teamRef)","assignee_ref":"\#(engineerRef)","source_message_refs":["\#(sourceRef)"],"requires_execution":true,"builtin_capabilities":["project_read","project_write"]}"#
        ))
        let persistedAfterCreate = try await store.listTeamTodos(
            ownerUserID: "alice",
            teamRoomID: room.id,
            includeTerminal: true
        )
        XCTAssertFalse(
            prerequisite.isError,
            "\(prerequisite.content); persisted=\(persistedAfterCreate.map(\.title))"
        )
        XCTAssertFalse(prerequisite.content.contains(engineer.id))
        XCTAssertFalse(prerequisite.content.contains(manager.id))

        let dependencyOptions = try await provider.execute(.init(
            id: "dependency-options",
            name: LocalAgentChatToolProvider.todoDependencyOptionsToolName,
            arguments: #"{"team_ref":"\#(teamRef)"}"#
        ))
        let dependencyJSON = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(dependencyOptions.content.utf8))
                as? [[String: Any]]
        )
        let prerequisiteRef = try XCTUnwrap(dependencyJSON.first?["todo_ref"] as? String)
        let invalidDependency = try await provider.execute(.init(
            id: "create-invalid-dependent",
            name: LocalAgentChatToolProvider.todoAddToolName,
            arguments: #"{"title":"伪造依赖","team_ref":"\#(teamRef)","assignee_ref":"\#(managerRef)","depends_on_todo_refs":["todo_forged"],"source_message_refs":["\#(sourceRef)"],"requires_execution":true,"builtin_capabilities":[]}"#
        ))
        XCTAssertTrue(invalidDependency.isError)
        XCTAssertTrue(invalidDependency.content.contains(#""code":"invalid_dependency_todo_ref""#))
        XCTAssertFalse(invalidDependency.content.contains(engineer.id))
        let dependent = try await provider.execute(.init(
            id: "create-dependent",
            name: LocalAgentChatToolProvider.todoAddToolName,
            arguments: #"{"title":"实现客户端","team_ref":"\#(teamRef)","assignee_ref":"\#(managerRef)","depends_on_todo_refs":["\#(prerequisiteRef)"],"source_message_refs":["\#(sourceRef)"],"requires_execution":true,"builtin_capabilities":["project_read","project_write"]}"#
        ))
        XCTAssertFalse(dependent.isError)
        let todos = try await store.listTeamTodos(
            ownerUserID: "alice",
            teamRoomID: room.id,
            includeTerminal: false
        )
        XCTAssertEqual(todos.count, 2)
        XCTAssertEqual(todos.first(where: { $0.title == "实现接口" })?.agentID, engineer.id)
        let dependentTodo = try XCTUnwrap(todos.first(where: { $0.title == "实现客户端" }))
        XCTAssertEqual(dependentTodo.agentID, manager.id)
        let dependencies = try await store.listAgentTodoDependencies(
            ownerUserID: "alice",
            agentID: manager.id,
            todoID: dependentTodo.id
        )
        XCTAssertEqual(dependencies.first?.prerequisiteAgentID, engineer.id)

        let workerMessage = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "检查任务板",
                mentionedAgentIDs: [engineer.id]
            ),
            limits: .init()
        )
        let workerClaimValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: engineer.id,
            nowUnixMs: workerMessage.message.createdAtUnixMs + 1
        )
        let workerClaim = try XCTUnwrap(workerClaimValue)
        let workerProvider = try await LocalAgentRelayMCPServer(service: service).connect(
            context: try .init(
                ownerUserID: "alice",
                projectID: room.projectID,
                roomID: room.id,
                agentID: engineer.id,
                deliveryID: workerClaim.id,
                triggerMessageID: workerClaim.messageID,
                rootMessageID: workerClaim.rootMessageID,
                runID: "worker-team-board-run",
                hopCount: 0
            )
        )
        let workerTools = Set(try await workerProvider.definitions().map(\.name))
        XCTAssertTrue(workerTools.contains(LocalAgentChatToolProvider.todoListToolName))
        XCTAssertFalse(workerTools.contains(LocalAgentChatToolProvider.todoAddToolName))
        XCTAssertFalse(workerTools.contains(LocalAgentChatToolProvider.todoUpdateToolName))
        XCTAssertFalse(workerTools.contains(LocalAgentChatToolProvider.todoReorderToolName))
        let workerInbox = try await workerProvider.execute(.init(
            id: "worker-read-inbox",
            name: LocalAgentChatToolProvider.readAllUnreadToolName,
            arguments: "{}"
        ))
        let workerInboxJSON = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(workerInbox.content.utf8))
                as? [String: Any]
        )
        let workerConversation = try XCTUnwrap(
            (workerInboxJSON["conversations"] as? [[String: Any]])?.first
        )
        let workerConversationRef = try XCTUnwrap(
            workerConversation["conversation_ref"] as? String
        )
        let workerSourceRef = try XCTUnwrap(
            (workerConversation["messages"] as? [[String: Any]])?.first?["message_ref"]
                as? String
        )
        let requestedManagement = try await workerProvider.execute(.init(
            id: "worker-request-management",
            name: LocalAgentChatToolProvider.inboxSendToolName,
            arguments: #"{"conversation_ref":"\#(workerConversationRef)","reply_to_message_ref":"\#(workerSourceRef)","content":"发现新增工作，请项目经理任务化。","notify_project_manager":true}"#
        ))
        XCTAssertFalse(requestedManagement.isError)
        XCTAssertTrue(requestedManagement.content.contains(#""notified_project_manager":true"#))
        XCTAssertFalse(requestedManagement.content.contains(manager.id))
    }

    func testOrdinaryAssigneeCanPersistProgressAndCompleteItsOwnTodo() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let service = NativeAgentGroupChatService(databaseURL: url)
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(name: "执行者", rolePrompt: "执行 Todo。", modelConfigID: "model")
        )
        XCTAssertNotEqual(agent.draft.professionKey, "project_manager")
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "todo-status-project",
            draft: .init(name: "状态项目")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let source = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "完成这项工作"),
            limits: .init()
        ).message
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "todo-status-test",
            draft: .init(
                title: "执行测试任务",
                teamRoomID: room.id,
                sourceRoomID: room.id,
                sourceMessageID: source.id,
                executionPlan: .init(
                    builtinCapabilities: [.projectRead]
                )
            ),
            nowUnixMs: source.createdAtUnixMs + 1
        )
        let direct = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        _ = try await store.postMessage(
            ownerUserID: "alice",
            roomID: direct.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "并行处理通讯消息"),
            limits: .init()
        )
        _ = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: source.createdAtUnixMs + 2
        )
        let firstClaimValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: source.createdAtUnixMs + 3
        )
        let firstClaim = try XCTUnwrap(firstClaimValue)
        let secondClaimValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: source.createdAtUnixMs + 3
        )
        let secondClaim = try XCTUnwrap(secondClaimValue)
        XCTAssertEqual(Set([firstClaim.lane, secondClaim.lane]), Set([.manager, .executor]))
        let executorDelivery = try XCTUnwrap(
            [firstClaim, secondClaim].first(where: { $0.lane == .executor })
        )
        let concurrentManagerDelivery = try XCTUnwrap(
            [firstClaim, secondClaim].first(where: { $0.lane == .manager })
        )
        XCTAssertEqual(executorDelivery.lane, .executor)
        let executor = try await LocalAgentRelayMCPServer(
            service: service,
            now: { source.createdAtUnixMs + 4 }
        ).connect(context: try .init(
            ownerUserID: "alice",
            projectID: room.projectID,
            roomID: room.id,
            agentID: agent.id,
            deliveryID: executorDelivery.id,
            triggerMessageID: executorDelivery.messageID,
            rootMessageID: executorDelivery.rootMessageID,
            runID: "executor-run",
            hopCount: 0,
            lane: .executor
        ))
        let executorDefinitions = try await executor.definitions()
        XCTAssertEqual(Set(executorDefinitions.map(\.name)), Set([
            "todo_get_context", "todo_progress_append", "todo_complete", "todo_block",
        ]))
        _ = try await executor.execute(.init(
            id: "progress",
            name: LocalAgentChatToolProvider.todoProgressAppendToolName,
            arguments: #"{"stage":"verification","detail":"已完成验证。"}"#
        ))
        _ = try await executor.execute(.init(
            id: "complete",
            name: LocalAgentChatToolProvider.todoCompleteToolName,
            arguments: #"{"summary":"任务已经完成并通过验证。"}"#
        ))
        _ = try await store.failDelivery(
            ownerUserID: "alice",
            deliveryID: concurrentManagerDelivery.id,
            error: "测试释放通讯 lane",
            nowUnixMs: source.createdAtUnixMs + 5
        )
        let completedTodo = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(completedTodo?.status, .completed)
        let managerDeliveryValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: source.createdAtUnixMs + 6
        )
        let managerDelivery = try XCTUnwrap(managerDeliveryValue)
        XCTAssertEqual(managerDelivery.triggerKind, .todoStatus)
        XCTAssertEqual(managerDelivery.lane, .manager)
        let managerRoomValue = try await store.room(
            ownerUserID: "alice",
            roomID: managerDelivery.roomID
        )
        let managerRoom = try XCTUnwrap(managerRoomValue)
        let manager = try await LocalAgentRelayMCPServer(
            service: service,
            now: { source.createdAtUnixMs + 7 }
        ).connect(context: try .init(
            ownerUserID: "alice",
            projectID: managerRoom.projectID,
            roomID: managerRoom.id,
            agentID: agent.id,
            deliveryID: managerDelivery.id,
            triggerMessageID: managerDelivery.messageID,
            rootMessageID: managerDelivery.rootMessageID,
            runID: "manager-run",
            hopCount: 0,
            lane: .manager
        ))
        let managerDefinitions = Set(try await manager.definitions().map(\.name))
        XCTAssertTrue(managerDefinitions.contains("agent_cycle_complete"))
        XCTAssertTrue(managerDefinitions.contains("todo_read_progress"))
        XCTAssertFalse(managerDefinitions.contains("todo_progress_append"))
        let listed = try await manager.execute(.init(
            id: "list-terminal",
            name: LocalAgentChatToolProvider.todoListToolName,
            arguments: #"{"include_terminal":true}"#
        ))
        let listedJSON = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(listed.content.utf8)) as? [[String: Any]]
        )
        let todoReference = try XCTUnwrap(listedJSON.first?["todo_ref"] as? String)
        let progress = try await manager.execute(.init(
            id: "read-progress",
            name: LocalAgentChatToolProvider.todoReadProgressToolName,
            arguments: #"{"todo_ref":"\#(todoReference)"}"#
        ))
        XCTAssertTrue(progress.content.contains("已完成验证"))
        XCTAssertTrue(progress.content.contains("任务已经完成"))
        _ = try await manager.execute(.init(
            id: "cycle-complete",
            name: LocalAgentChatToolProvider.completeManagerCycleToolName,
            arguments: "{}"
        ))
        let completedManagerDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: managerDelivery.id
        )
        XCTAssertEqual(completedManagerDelivery?.status, .completed)
        let directTranscript = try await store.listMessages(
            ownerUserID: "alice",
            roomID: managerRoom.id,
            limit: 20
        )
        XCTAssertEqual(directTranscript.map(\.content), ["并行处理通讯消息"])
    }
}
