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

/// In-process MCP-compatible chat surface. A future stdio/HTTP MCP adapter should delegate to
/// this same store/provider instead of duplicating room authorization or delivery semantics.
public struct LocalAgentChatToolProvider: AgentToolProvider, Sendable {
    public static let getTriggerToolName = "chat_get_trigger"
    public static let listMembersToolName = "chat_list_members"
    public static let readMessagesToolName = "chat_read_messages"
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
        case Self.getTriggerToolName:
            return try await getTrigger(call)
        case Self.listMembersToolName:
            return try await listMembers(call)
        case Self.readMessagesToolName:
            return try await readMessages(call)
        case Self.sendMessageToolName:
            return try await sendMessage(call)
        default:
            return .failure("群聊工具不可用：\(call.name)")
        }
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

    private func readMessages(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let afterUnixMs = try Self.optionalInteger(arguments, key: "after_unix_ms")
        let limit = try Self.optionalInteger(arguments, key: "limit").map(Int.init) ?? 50
        let messages = try await store.listMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            afterUnixMs: afterUnixMs,
            limit: limit
        )
        return try Self.outcome(messages)
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
        return try Self.outcome(
            SendResponse(
                messageID: post.message.id,
                deliveryID: completed.id,
                spawnedDeliveryIDs: post.deliveries.map(\.id),
                routingStopReason: post.routingStopReason
            )
        )
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
            name: readMessagesToolName,
            description: "按时间读取当前项目群聊消息；不要一次请求不必要的大量历史。",
            schema: Data(#"{"type":"object","properties":{"after_unix_ms":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: sendMessageToolName,
            description: "以当前 Agent 身份回复群聊，可用稳定 Agent ID 提及其他成员。成功发送即完成当前 delivery。",
            schema: Data(#"{"type":"object","properties":{"content":{"type":"string","minLength":1,"maxLength":64000},"mention_agent_ids":{"type":"array","items":{"type":"string","minLength":1,"maxLength":512},"maxItems":32,"uniqueItems":true},"reply_to_message_id":{"type":"string","minLength":1,"maxLength":512}},"required":["content"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
    ]
}
