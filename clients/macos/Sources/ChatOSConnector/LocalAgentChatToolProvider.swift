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

    public func connect(
        context: LocalAgentChatRunContext,
        professions: [LocalAgentProfessionDefinition] = LocalAgentSkillCatalog.professions
    ) async throws -> LocalAgentChatToolProvider {
        let store = try await service.store()
        return try LocalAgentChatToolProvider(
            store: store,
            context: context,
            professions: professions,
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
    public static let readAttachmentToolName = "chat_read_attachment"
    public static let markReadToolName = "chat_mark_read"
    public static let openDirectToolName = "chat_direct_open"
    public static let sendDirectToolName = "chat_direct_send"
    public static let proposeMemberToolName = "agent_propose_member"
    public static let proposeMemberRemovalToolName = "agent_propose_member_removal"
    public static let sendMessageToolName = "chat_send_message"

    private let store: any AgentGroupChatStore
    private let context: LocalAgentChatRunContext
    private let professions: [LocalAgentProfessionDefinition]
    private let limits: AgentGroupChatRoutingLimits
    private let now: @Sendable () -> Int64

    public init(
        store: any AgentGroupChatStore,
        context: LocalAgentChatRunContext,
        professions: [LocalAgentProfessionDefinition] = LocalAgentSkillCatalog.professions,
        limits: AgentGroupChatRoutingLimits = .init(),
        now: @escaping @Sendable () -> Int64 = {
            Int64(Date().timeIntervalSince1970 * 1_000)
        }
    ) throws {
        try limits.validate()
        self.store = store
        self.context = context
        self.professions = professions
        self.limits = limits
        self.now = now
    }

    public func definitions() async throws -> [AgentToolDefinition] {
        var definitions = Self.toolDefinitions
        if !(try await canManageStaff()) {
            definitions = definitions.filter {
                $0.name != Self.proposeMemberToolName
                    && $0.name != Self.proposeMemberRemovalToolName
            }
        } else {
            definitions.removeAll { $0.name == Self.proposeMemberToolName }
            definitions.append(try memberProposalDefinition())
            if try await store.room(
                ownerUserID: context.ownerUserID,
                roomID: context.roomID
            )?.conversationKind.isDirect == true {
                definitions = definitions.filter { $0.name != Self.proposeMemberRemovalToolName }
            }
        }
        return definitions
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
        case Self.readAttachmentToolName:
            return try await readAttachment(call)
        case Self.markReadToolName:
            return try await markRead(call)
        case Self.openDirectToolName:
            return try await openDirect(call)
        case Self.sendDirectToolName:
            return try await sendDirect(call)
        case Self.proposeMemberToolName:
            return try await proposeMember(call)
        case Self.proposeMemberRemovalToolName:
            return try await proposeMemberRemoval(call)
        case Self.sendMessageToolName:
            return try await sendMessage(call)
        default:
            return .failure("群聊工具不可用：\(call.name)")
        }
    }

    private func bootstrap(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        guard let room = try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID
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

    private func readAttachment(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let messageID = try Self.requiredString(arguments, key: "message_id")
        let attachmentID = try Self.requiredString(arguments, key: "attachment_id")
        let offset = max(0, Int(try Self.optionalInteger(arguments, key: "offset") ?? 0))
        let limit = min(
            12_000,
            max(1, Int(try Self.optionalInteger(arguments, key: "limit") ?? 12_000))
        )
        guard let payload = try await store.messageAttachment(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            messageID: messageID,
            attachmentID: attachmentID
        ) else { throw AgentGroupChatError.notFound }
        let data = try Data(contentsOf: payload.localFileURL, options: [.mappedIfSafe])
        var response: [String: NativeJSONValue] = [
            "message_id": .string(messageID),
            "attachment_id": .string(payload.attachment.id),
            "name": .string(payload.attachment.name),
            "mime_type": .string(payload.attachment.mimeType),
            "kind": .string(payload.attachment.kind.rawValue),
            "size": .number(Double(payload.attachment.size)),
        ]
        if !data.prefix(8_000).contains(0), let text = String(data: data, encoding: .utf8) {
            let characters = Array(text)
            let start = min(offset, characters.count)
            let end = min(start + limit, characters.count)
            response["content"] = .string(String(characters[start..<end]))
            response["offset"] = .number(Double(start))
            response["next_offset"] = end < characters.count ? .number(Double(end)) : .null
            response["has_more"] = .bool(end < characters.count)
        } else {
            response["content"] = .null
            response["multimodal_on_trigger"] = .bool(messageID == context.triggerMessageID)
            response["note"] = .string(
                messageID == context.triggerMessageID
                    ? "该二进制附件已作为当前触发消息的多模态输入提供给模型。"
                    : "该二进制附件不能作为文本读取；请让 Human 在新消息中重新附带，或使用匹配的本机 Plugin。"
            )
        }
        return try Self.outcome(response)
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

    private func openDirect(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let targetAgentID = try Self.requiredString(arguments, key: "target_agent_id")
        let conversation = try await store.openAgentDirect(
            ownerUserID: context.ownerUserID,
            initiatingAgentID: context.agentID,
            targetAgentID: targetAgentID
        )
        return try Self.outcome(DirectOpenResponse(
            conversationID: conversation.id,
            targetAgentID: targetAgentID
        ))
    }

    private func sendDirect(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let conversationID = try Self.requiredString(arguments, key: "conversation_id")
        let content = try Self.requiredString(arguments, key: "content")
        guard let conversation = try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: conversationID
        ), conversation.conversationKind == .agentAgentDirect else {
            throw AgentGroupChatError.notFound
        }
        let members = try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: conversationID
        )
        guard members.contains(where: { $0.agentID == context.agentID }) else {
            throw AgentGroupChatError.notMember
        }
        let post = try await store.postMessage(
            ownerUserID: context.ownerUserID,
            roomID: conversationID,
            draft: .init(
                senderKind: .agent,
                senderID: context.agentID,
                content: content,
                sourceRunID: context.runID,
                causationID: context.deliveryID,
                hopCount: context.hopCount + 1
            ),
            limits: limits
        )
        return try Self.outcome(DirectSendResponse(
            conversationID: conversationID,
            messageID: post.message.id,
            spawnedDeliveryIDs: post.deliveries.map(\.id),
            routingStopReason: post.routingStopReason
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
        guard try await canManageStaff() else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        guard let currentProfile = profiles.first(where: { $0.id == context.agentID }) else {
            throw AgentGroupChatError.notFound
        }
        let requestedModelConfigID = try Self.optionalString(
            arguments,
            key: "model_config_id"
        )?.trimmingCharacters(in: .whitespacesAndNewlines)
        let modelConfigID = requestedModelConfigID.flatMap {
            $0.isEmpty || $0.caseInsensitiveCompare("default") == .orderedSame ? nil : $0
        } ?? currentProfile.draft.modelConfigID
        let requestedThinkingLevel = try Self.optionalString(
            arguments,
            key: "thinking_level"
        )?.trimmingCharacters(in: .whitespacesAndNewlines)
        let thinkingLevel = requestedThinkingLevel.flatMap { $0.isEmpty ? nil : $0 }
            ?? currentProfile.draft.thinkingLevel
        let draft = LocalAgentDraft(
            name: try Self.requiredString(arguments, key: "name"),
            role: try Self.requiredString(arguments, key: "role"),
            responsibility: try Self.optionalString(arguments, key: "responsibility") ?? "",
            rolePrompt: try Self.requiredString(arguments, key: "role_prompt"),
            modelConfigID: modelConfigID,
            thinkingLevel: thinkingLevel,
            professionKey: try Self.requiredString(arguments, key: "profession_key"),
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

    private func proposeMemberRemoval(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard try await canManageStaff() else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let proposal = try await store.createAgentRemovalProposal(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            proposerAgentID: context.agentID,
            sourceDeliveryID: context.deliveryID,
            requestKey: call.id,
            draft: .init(
                targetAgentID: try Self.requiredString(arguments, key: "target_agent_id"),
                reason: try Self.requiredString(arguments, key: "reason"),
                handoffPlan: try Self.optionalString(arguments, key: "handoff_plan") ?? ""
            ),
            nowUnixMs: now()
        )
        return try Self.outcome(proposal)
    }

    private func canManageStaff() async throws -> Bool {
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        guard let current = profiles.first(where: { $0.id == context.agentID }) else {
            throw AgentGroupChatError.notFound
        }
        return LocalAgentPermission.canManageStaff(current.draft.defaultSkillIDs)
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
        let roomID: String
        let roomName: String
        let roomGoal: String
        let deliveryID: String
        let trigger: ProjectAgentMessage
        let unread: ProjectAgentUnreadPage
        let members: [MemberResponse]

        enum CodingKeys: String, CodingKey {
            case agent
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

    private struct DirectOpenResponse: Encodable {
        let conversationID: String
        let targetAgentID: String

        enum CodingKeys: String, CodingKey {
            case conversationID = "conversation_id"
            case targetAgentID = "target_agent_id"
        }
    }

    private struct DirectSendResponse: Encodable {
        let conversationID: String
        let messageID: String
        let spawnedDeliveryIDs: [String]
        let routingStopReason: String?

        enum CodingKeys: String, CodingKey {
            case conversationID = "conversation_id"
            case messageID = "message_id"
            case spawnedDeliveryIDs = "spawned_delivery_ids"
            case routingStopReason = "routing_stop_reason"
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
            name: readAttachmentToolName,
            description: "按消息和附件 ID 读取当前会话附件。文本可用 offset/limit 分段读取；当前触发消息中的图片或 PDF 会由客户端直接作为多模态输入交给模型。",
            schema: Data(#"{"type":"object","properties":{"message_id":{"type":"string","minLength":1,"maxLength":512},"attachment_id":{"type":"string","minLength":1,"maxLength":512},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":12000}},"required":["message_id","attachment_id"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: markReadToolName,
            description: "把当前 Agent 的独立已读游标推进到指定消息。游标单调前进，旧调用或重试不会把已读位置回退。",
            schema: Data(#"{"type":"object","properties":{"through_message_id":{"type":"string","minLength":1,"maxLength":512}},"required":["through_message_id"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: openDirectToolName,
            description: "打开或复用与另一个 Agent 的私聊。不能与自己私聊；A 到 B 和 B 到 A 会得到同一个 conversation_id。",
            schema: Data(#"{"type":"object","properties":{"target_agent_id":{"type":"string","minLength":1,"maxLength":512}},"required":["target_agent_id"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: sendDirectToolName,
            description: "向已经打开的 Agent 私聊发送消息。当前 Agent 必须是该私聊参与者，成功后会通过本地 delivery 唤醒对方。",
            schema: Data(#"{"type":"object","properties":{"conversation_id":{"type":"string","minLength":1,"maxLength":512},"content":{"type":"string","minLength":1,"maxLength":64000}},"required":["conversation_id","content"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: Self.proposeMemberToolName,
            description: "使用已授予的人员管理权限，向 Human 提交一个新 Agent 草案。该工具只持久化待确认提案，绝不会直接创建 Agent；私聊中确认后只创建独立 Agent，团队会话中确认后才加入当前团队。model_config_id 和 thinking_level 省略时继承当前 Agent。",
            schema: Data(#"{"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":120},"role":{"type":"string","minLength":1,"maxLength":160},"responsibility":{"type":"string","maxLength":8000},"role_prompt":{"type":"string","minLength":1,"maxLength":32000},"model_config_id":{"type":"string","minLength":1,"maxLength":512},"thinking_level":{"type":"string","enum":["auto","none","minimal","low","medium","high","xhigh","max"]},"rationale":{"type":"string","maxLength":4000}},"required":["name","role","role_prompt"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: proposeMemberRemovalToolName,
            description: "使用已授予的人员管理权限，向 Human 提交把一个 Agent 移出当前项目团队的提案。必须提供事实理由和可选交接计划；该工具不会删除可复用的 Agent profile，也不会绕过 Human 确认。",
            schema: Data(#"{"type":"object","properties":{"target_agent_id":{"type":"string","minLength":1,"maxLength":512},"reason":{"type":"string","minLength":1,"maxLength":4000},"handoff_plan":{"type":"string","maxLength":8000}},"required":["target_agent_id","reason"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: sendMessageToolName,
            description: "以当前 Agent 身份回复群聊，可用稳定 Agent ID 提及其他成员。成功发送即完成当前 delivery。",
            schema: Data(#"{"type":"object","properties":{"content":{"type":"string","minLength":1,"maxLength":64000},"mention_agent_ids":{"type":"array","items":{"type":"string","minLength":1,"maxLength":512},"maxItems":32,"uniqueItems":true},"reply_to_message_id":{"type":"string","minLength":1,"maxLength":512}},"required":["content"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
    ]

    private func memberProposalDefinition() throws -> AgentToolDefinition {
        let schema: [String: Any] = [
            "type": "object",
            "properties": [
                "name": ["type": "string", "minLength": 1, "maxLength": 120],
                "role": ["type": "string", "minLength": 1, "maxLength": 160],
                "responsibility": ["type": "string", "maxLength": 8_000],
                "role_prompt": ["type": "string", "minLength": 1, "maxLength": 32_000],
                "model_config_id": ["type": "string", "minLength": 1, "maxLength": 512],
                "thinking_level": [
                    "type": "string",
                    "enum": LocalAgentThinkingLevelCatalog.allValues.sorted(),
                    "description": "省略时继承当前 Agent 的思考等级；确认创建时会按实际模型能力重新校验。",
                ],
                "profession_key": [
                    "type": "string",
                    "enum": professions.map(\.key),
                    "description": professions.map { "\($0.key)=\($0.label)" }.joined(separator: "；"),
                ],
                "rationale": ["type": "string", "maxLength": 4_000],
            ],
            "required": ["name", "role", "role_prompt", "profession_key"],
            "additionalProperties": false,
        ]
        return .init(
            name: Self.proposeMemberToolName,
            description: "使用已授予的人员管理权限，向 Human 提交一个新 Agent 草案。必须从客户端目录选择职业；model_config_id 和 thinking_level 省略时继承当前 Agent。该工具只持久化待确认提案，绝不会直接创建 Agent。",
            schema: try JSONSerialization.data(withJSONObject: schema, options: [.sortedKeys]),
            effect: .write
        )
    }
}
