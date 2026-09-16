import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// Immutable authority for one claimed local delivery. Caller-controlled tool arguments never
/// select the sender, account, project, room or Memory identity.
public struct LocalAgentChatRunContext: Codable, Sendable, Equatable {
    public let ownerUserID: String
    public let projectID: String
    public let roomID: String
    public let agentID: String
    public let deliveryID: String
    public let triggerMessageID: String
    public let rootMessageID: String
    public let runID: String
    public let hopCount: Int

    public init(
        ownerUserID: String,
        projectID: String,
        roomID: String,
        agentID: String,
        deliveryID: String,
        triggerMessageID: String,
        rootMessageID: String,
        runID: String,
        hopCount: Int
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        try AgentGroupChatValidation.identifier(triggerMessageID, field: "triggerMessageID")
        try AgentGroupChatValidation.identifier(rootMessageID, field: "rootMessageID")
        try AgentGroupChatValidation.identifier(runID, field: "runID")
        guard (0...64).contains(hopCount) else {
            throw AgentGroupChatError.invalidField("hopCount")
        }
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.roomID = roomID
        self.agentID = agentID
        self.deliveryID = deliveryID
        self.triggerMessageID = triggerMessageID
        self.rootMessageID = rootMessageID
        self.runID = runID
        self.hopCount = hopCount
    }
}

/// The single local collaboration MCP shared by every native Agent. It owns no model runtime:
/// each connection is scoped to one immutable Agent delivery identity and all communication
/// goes through the same durable room store. A future stdio/HTTP transport can delegate to this
/// server without changing its authorization or routing semantics.
public actor LocalAgentRelayMCPServer {
    private let service: NativeAgentGroupChatService
    private let limits: AgentGroupChatRoutingLimits
    private let now: @Sendable () -> Int64

    public init(
        service: NativeAgentGroupChatService,
        limits: AgentGroupChatRoutingLimits = .init(),
        now: @escaping @Sendable () -> Int64 = {
            Int64(Date().timeIntervalSince1970 * 1_000)
        }
    ) {
        self.service = service
        self.limits = limits
        self.now = now
    }

    public func connect(context: LocalAgentChatRunContext) async throws -> LocalAgentChatToolProvider {
        let store = try await service.store()
        return try LocalAgentChatToolProvider(
            store: store,
            context: context,
            limits: limits,
            now: now
        )
    }
}

/// One identity-bound session on the local Relay MCP. Tool arguments can never select another
/// account, project, room, Agent, delivery or Memory identity.
public struct LocalAgentChatToolProvider: AgentToolProvider, Sendable {
    public static let bootstrapToolName = "relay_bootstrap"
    public static let getTriggerToolName = "chat_get_trigger"
    public static let listMembersToolName = "chat_list_members"
    public static let readUnreadToolName = "chat_read_unread"
    public static let readMessagesToolName = "chat_read_messages"
    public static let markReadToolName = "chat_mark_read"
    public static let proposeMemberToolName = "agent_propose_member"
    public static let sendMessageToolName = "chat_send_message"

    private let store: any AgentGroupChatStore
    private let context: LocalAgentChatRunContext
    private let limits: AgentGroupChatRoutingLimits
    private let now: @Sendable () -> Int64

    public init(
        store: any AgentGroupChatStore,
        context: LocalAgentChatRunContext,
        limits: AgentGroupChatRoutingLimits = .init(),
        now: @escaping @Sendable () -> Int64 = {
            Int64(Date().timeIntervalSince1970 * 1_000)
        }
    ) throws {
        try limits.validate()
        self.store = store
        self.context = context
        self.limits = limits
        self.now = now
    }

    public func definitions() async throws -> [AgentToolDefinition] {
        Self.toolDefinitions
    }

    public func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        switch call.name {
        case Self.bootstrapToolName:
            return try await bootstrap(call)
        case Self.getTriggerToolName:
            return try await getTrigger(call)
        case Self.listMembersToolName:
            return try await listMembers(call)
        case Self.readUnreadToolName:
            return try await readUnread(call)
        case Self.readMessagesToolName:
            return try await readMessages(call)
        case Self.markReadToolName:
            return try await markRead(call)
        case Self.proposeMemberToolName:
            return try await proposeMember(call)
        case Self.sendMessageToolName:
            return try await sendMessage(call)
        default:
            return .failure("群聊工具不可用：\(call.name)")
        }
    }

    private func bootstrap(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        guard let room = try await store.activeRoom(
            ownerUserID: context.ownerUserID,
            projectID: context.projectID
        ), room.id == context.roomID,
           let trigger = try await store.message(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            messageID: context.triggerMessageID
           ) else { throw AgentGroupChatError.notFound }
        let members = try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID
        )
        guard let currentMember = members.first(where: { $0.agentID == context.agentID }) else {
            throw AgentGroupChatError.notMember
        }
        let agents = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        let profiles = Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
        guard let currentProfile = profiles[context.agentID] else {
            throw AgentGroupChatError.notFound
        }
        let unread = try await store.listUnreadMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            limit: 20
        )
        return try Self.outcome(BootstrapResponse(
            agent: .init(
                agentID: currentProfile.id,
                name: currentProfile.draft.name,
                role: currentMember.draft.role,
                responsibility: currentMember.draft.responsibility
            ),
            projectID: context.projectID,
            roomID: room.id,
            roomName: room.draft.name,
            roomGoal: room.draft.goal,
            deliveryID: context.deliveryID,
            trigger: trigger,
            unread: unread,
            members: members.map { member in
                MemberResponse(
                    agentID: member.agentID,
                    name: profiles[member.agentID]?.draft.name ?? member.agentID,
                    role: member.draft.role,
                    responsibility: member.draft.responsibility
                )
            }
        ))
    }

    private func getTrigger(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        guard let message = try await store.message(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            messageID: context.triggerMessageID
        ) else { throw AgentGroupChatError.notFound }
        return try Self.outcome(message)
    }

    private func listMembers(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        let members = try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID
        )
        let agents = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        let profiles = Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
        let response = members.map { member in
            MemberResponse(
                agentID: member.agentID,
                name: profiles[member.agentID]?.draft.name ?? member.agentID,
                role: member.draft.role,
                responsibility: member.draft.responsibility
            )
        }
        return try Self.outcome(response)
    }

    private func readUnread(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let limit = try Self.optionalInteger(arguments, key: "limit").map(Int.init) ?? 50
        let page = try await store.listUnreadMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            limit: limit
        )
        return try Self.outcome(page)
    }

    private func readMessages(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let afterMessageID = try Self.optionalString(arguments, key: "after_message_id")
        let limit = try Self.optionalInteger(arguments, key: "limit").map(Int.init) ?? 50
        let page = try await store.pageMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            afterMessageID: afterMessageID,
            limit: limit
        )
        return try Self.outcome(page)
    }

    private func markRead(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let throughMessageID = try Self.requiredString(arguments, key: "through_message_id")
        let cursor = try await store.markMessagesRead(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            throughMessageID: throughMessageID,
            nowUnixMs: now()
        )
        let remaining = try await store.listUnreadMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            limit: 1
        )
        return try Self.outcome(MarkReadResponse(
            cursor: cursor,
            hasUnread: !remaining.messages.isEmpty,
            nextUnreadMessageID: remaining.messages.first?.id
        ))
    }

    private func sendMessage(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let content = try Self.requiredString(arguments, key: "content")
        let mentionAgentIDs = try Self.optionalStringArray(arguments, key: "mention_agent_ids")
        let replyToMessageID = try Self.optionalString(arguments, key: "reply_to_message_id")
            ?? context.triggerMessageID
        guard let delivery = try await store.delivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), delivery.status == .running,
           delivery.roomID == context.roomID,
           delivery.targetAgentID == context.agentID,
           delivery.messageID == context.triggerMessageID,
           delivery.rootMessageID == context.rootMessageID else {
            throw AgentGroupChatError.conflict
        }
        let post = try await store.postMessage(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            draft: .init(
                senderKind: .agent,
                senderID: context.agentID,
                content: content,
                mentionedAgentIDs: mentionAgentIDs,
                replyToMessageID: replyToMessageID,
                sourceRunID: context.runID,
                causationID: context.deliveryID,
                rootMessageID: context.rootMessageID,
                hopCount: context.hopCount + 1
            ),
            limits: limits
        )
        let completed = try await store.completeDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID,
            responseMessageID: post.message.id,
            nowUnixMs: now()
        )
        // A substantive reply acknowledges the triggering message. This best-effort cursor update
        // is intentionally secondary to the already durable message/delivery transaction.
        _ = try? await store.markMessagesRead(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            throughMessageID: context.triggerMessageID,
            nowUnixMs: now()
        )
        return try Self.outcome(
            SendResponse(
                messageID: post.message.id,
                deliveryID: completed.id,
                spawnedDeliveryIDs: post.deliveries.map(\.id),
                routingStopReason: post.routingStopReason
            )
        )
    }

    private func proposeMember(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        guard let currentProfile = profiles.first(where: { $0.id == context.agentID }) else {
            throw AgentGroupChatError.notFound
        }
        let draft = LocalAgentDraft(
            name: try Self.requiredString(arguments, key: "name"),
            role: try Self.requiredString(arguments, key: "role"),
            responsibility: try Self.optionalString(arguments, key: "responsibility") ?? "",
            rolePrompt: try Self.requiredString(arguments, key: "role_prompt"),
            modelConfigID: try Self.optionalString(arguments, key: "model_config_id")
                ?? currentProfile.draft.modelConfigID,
            pluginIDs: try Self.optionalStringArray(arguments, key: "plugin_ids"),
            rationale: try Self.optionalString(arguments, key: "rationale") ?? ""
        )
        try draft.validate()
        let proposal = try await store.createAgentProposal(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            proposerAgentID: context.agentID,
            sourceDeliveryID: context.deliveryID,
            requestKey: call.id,
            draft: draft,
            nowUnixMs: now()
        )
        return try Self.outcome(proposal)
    }

    private struct MemberResponse: Encodable {
        let agentID: String
        let name: String
        let role: String
        let responsibility: String

        enum CodingKeys: String, CodingKey {
            case agentID = "agent_id"
            case name, role, responsibility
        }
    }

    private struct BootstrapResponse: Encodable {
        let agent: MemberResponse
        let projectID: String
        let roomID: String
        let roomName: String
        let roomGoal: String
        let deliveryID: String
        let trigger: ProjectAgentMessage
        let unread: ProjectAgentUnreadPage
        let members: [MemberResponse]

        enum CodingKeys: String, CodingKey {
            case agent
            case projectID = "project_id"
            case roomID = "room_id"
            case roomName = "room_name"
            case roomGoal = "room_goal"
            case deliveryID = "delivery_id"
            case trigger, unread, members
        }
    }

    private struct MarkReadResponse: Encodable {
        let cursor: ProjectAgentReadCursor
        let hasUnread: Bool
        let nextUnreadMessageID: String?

        enum CodingKeys: String, CodingKey {
            case cursor
            case hasUnread = "has_unread"
            case nextUnreadMessageID = "next_unread_message_id"
        }
    }

    private struct SendResponse: Encodable {
        let messageID: String
        let deliveryID: String
        let spawnedDeliveryIDs: [String]
        let routingStopReason: String?

        enum CodingKeys: String, CodingKey {
            case messageID = "message_id"
            case deliveryID = "delivery_id"
            case spawnedDeliveryIDs = "spawned_delivery_ids"
            case routingStopReason = "routing_stop_reason"
        }
    }

    private static func arguments(_ call: AgentToolCall) throws -> [String: Any] {
        guard let data = call.arguments.data(using: .utf8),
              let value = try? JSONSerialization.jsonObject(with: data),
              let object = value as? [String: Any] else {
            throw AgentGroupChatError.invalidField("arguments")
        }
        return object
    }

    private static func requiredString(_ object: [String: Any], key: String) throws -> String {
        guard let value = object[key] as? String else {
            throw AgentGroupChatError.invalidField(key)
        }
        return value
    }

    private static func optionalString(_ object: [String: Any], key: String) throws -> String? {
        guard let value = object[key] else { return nil }
        guard !(value is NSNull), let string = value as? String else {
            throw AgentGroupChatError.invalidField(key)
        }
        return string
    }

    private static func optionalInteger(_ object: [String: Any], key: String) throws -> Int64? {
        guard let value = object[key] else { return nil }
        guard !(value is NSNull), let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else {
            throw AgentGroupChatError.invalidField(key)
        }
        let integer = number.int64Value
        guard number.doubleValue == Double(integer) else {
            throw AgentGroupChatError.invalidField(key)
        }
        return integer
    }

    private static func optionalStringArray(_ object: [String: Any], key: String) throws -> [String] {
        guard let value = object[key] else { return [] }
        guard let values = value as? [String] else {
            throw AgentGroupChatError.invalidField(key)
        }
        return values
    }

    private static func outcome<Value: Encodable>(_ value: Value) throws -> AgentToolOutcome {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return .init(String(decoding: try encoder.encode(value), as: UTF8.self))
    }

    private static let toolDefinitions: [AgentToolDefinition] = [
        .init(
            name: bootstrapToolName,
            description: "连接本地 Relay MCP 后读取当前 Agent 身份、绑定项目、团队、成员和本次唤醒消息。身份与范围由客户端固定，不能由参数切换。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: getTriggerToolName,
            description: "读取唤醒当前 Agent 的群聊消息。项目、房间和消息身份由运行上下文固定。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: listMembersToolName,
            description: "列出当前项目群聊中的 Agent 成员及职责。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: readUnreadToolName,
            description: "读取当前 Agent 在这个群聊中的未读消息。已读位置按 Agent 独立持久化；读取不会自动确认，处理后调用 chat_mark_read。",
            schema: Data(#"{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: readMessagesToolName,
            description: "使用稳定消息 ID 游标分页读取当前项目群聊记录；响应中的 next_cursor_message_id 可用于下一页。",
            schema: Data(#"{"type":"object","properties":{"after_message_id":{"type":"string","minLength":1,"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: markReadToolName,
            description: "把当前 Agent 的独立已读游标推进到指定消息。游标单调前进，旧调用或重试不会把已读位置回退。",
            schema: Data(#"{"type":"object","properties":{"through_message_id":{"type":"string","minLength":1,"maxLength":512}},"required":["through_message_id"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: proposeMemberToolName,
            description: "向 Human 提交一个新 Agent 成员草案。该工具只持久化待确认提案，绝不会直接创建 Agent；账号、项目、团队和提案者身份由当前 Relay session 固定。model_config_id 省略时继承当前 Agent，Plugin 必须在 Human 确认时仍已安装可用。",
            schema: Data(#"{"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":120},"role":{"type":"string","minLength":1,"maxLength":160},"responsibility":{"type":"string","maxLength":8000},"role_prompt":{"type":"string","minLength":1,"maxLength":32000},"model_config_id":{"type":"string","minLength":1,"maxLength":512},"plugin_ids":{"type":"array","items":{"type":"string","minLength":1,"maxLength":512},"maxItems":100,"uniqueItems":true},"rationale":{"type":"string","maxLength":4000}},"required":["name","role","role_prompt"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: sendMessageToolName,
            description: "以当前 Agent 身份回复群聊，可用稳定 Agent ID 提及其他成员。成功发送即完成当前 delivery。",
            schema: Data(#"{"type":"object","properties":{"content":{"type":"string","minLength":1,"maxLength":64000},"mention_agent_ids":{"type":"array","items":{"type":"string","minLength":1,"maxLength":512},"maxItems":32,"uniqueItems":true},"reply_to_message_id":{"type":"string","minLength":1,"maxLength":512}},"required":["content"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
    ]
}
