import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    nonisolated static func isCompanionRelayMessageType(_ messageType: String) -> Bool {
        switch messageType {
        case "companion_resources_request",
             "companion_resolve_resource_request",
             "companion_conversation_request",
             "companion_conversation_history_request",
             "companion_conversation_state_request",
             "companion_conversation_send_request",
             "companion_conversation_guidance_request",
             "companion_conversation_stop_request",
             "companion_message_tasks_request",
             "companion_ask_user_prompts_request",
             "companion_ask_user_submit_request",
             "companion_ask_user_cancel_request",
             "companion_agent_workspace_request",
             "companion_agent_conversation_request",
             "companion_agent_messages_request",
             "companion_agent_send_message_request",
             "companion_agent_open_direct_request",
             "companion_approvals_request",
             "companion_resolve_approval_request":
            true
        default:
            false
        }
    }

    func handleCompanionRelayMessage(
        _ data: Data,
        socket: URLSessionWebSocketTask
    ) async {
        let decoded = try? JSONDecoder().decode(NativeRelayRequest.self, from: data)
        let requestID = decoded?.requestID ?? ""
        let responseType = Self.companionResponseType(for: decoded?.type)
        do {
            guard let request = decoded else { throw NativeCompanionRelayError.unsupportedRequest }
            let response = try await processCompanionRelay(request)
            try await sendRelayResponse(response, socket: socket)
        } catch {
            let status: Int
            if let companionError = error as? NativeCompanionRelayError {
                status = companionError.status
            } else if let agentError = error as? AgentGroupChatError {
                status = switch agentError {
                case .notFound: 404
                case .permissionDenied, .notMember: 403
                case .invalidField, .conflict: 400
                case .storage: 500
                }
            } else {
                status = 500
            }
            let response = NativeRelayResponse(
                type: responseType,
                requestID: requestID,
                status: status,
                body: .object(["error": .string(error.localizedDescription)])
            )
            try? await sendRelayResponse(response, socket: socket)
        }
    }

    private func processCompanionRelay(
        _ request: NativeRelayRequest
    ) async throws -> NativeRelayResponse {
        guard let ownerUserID = state.user?.id,
              let deviceID = state.deviceID else {
            throw NativeCompanionRelayError.invalidContext
        }
        let runtimeConfig = try await managedRuntimeConfig()
        try NativeRelayVerifier().verify(
            request,
            trust: runtimeConfig.remoteControlTrust,
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            seenNonces: &seenRelayNonces
        )
        let body: NativeJSONValue
        switch request.type {
        case "companion_resources_request":
            guard let companionRuntime else {
                throw NativeCompanionRelayError.runtimeUnavailable
            }
            let resources = await companionRuntime.companionResources()
            body = try Self.nativeJSON(resources)
        case "companion_resolve_resource_request":
            guard let companionRuntime else {
                throw NativeCompanionRelayError.runtimeUnavailable
            }
            let payload = try request.body.decode(CompanionResolveRequest.self)
            let id = payload.resourceID.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !id.isEmpty else { throw NativeCompanionRelayError.missingResourceID }
            let resource = try await companionRuntime.resolveCompanionResource(id: id)
            body = try Self.nativeJSON(resource)
        case "companion_conversation_request":
            let payload = try request.body.decode(CompanionConversationRequest.self)
            body = try await companionConversation(payload, ownerUserID: ownerUserID)
        case "companion_conversation_history_request":
            let payload = try request.body.decode(CompanionConversationHistoryRequest.self)
            body = try await companionConversationHistory(payload, ownerUserID: ownerUserID)
        case "companion_conversation_state_request":
            let payload = try request.body.decode(CompanionConversationRequest.self)
            body = try await companionConversationState(payload, ownerUserID: ownerUserID)
        case "companion_conversation_send_request":
            let payload = try request.body.decode(CompanionConversationTurnRequest.self)
            body = try await sendCompanionConversationTurn(payload, guidance: false)
        case "companion_conversation_guidance_request":
            let payload = try request.body.decode(CompanionConversationTurnRequest.self)
            body = try await sendCompanionConversationTurn(payload, guidance: true)
        case "companion_conversation_stop_request":
            let payload = try request.body.decode(CompanionConversationStopRequest.self)
            guard let service = companionConversationService else {
                throw NativeCompanionRelayError.localAgentRuntimeUnavailable
            }
            try await service.stopTurn(
                conversationID: payload.conversationID,
                turnID: payload.turnID
            )
            body = .object(["success": .bool(true)])
        case "companion_message_tasks_request":
            let payload = try request.body.decode(CompanionMessageTasksRequest.self)
            body = try await companionMessageTasks(payload, ownerUserID: ownerUserID)
        case "companion_ask_user_prompts_request":
            let payload = try request.body.decode(CompanionAskUserPromptsRequest.self)
            guard let service = companionAskUserService else {
                throw NativeCompanionRelayError.localAgentRuntimeUnavailable
            }
            let prompts = try await service.fetchPrompts(
                sessionID: payload.conversationID,
                limit: min(100, max(1, payload.limit ?? 100))
            )
            body = .object([
                "success": .bool(true),
                "prompts": .array(prompts.map(Self.companionAskUserPrompt)),
            ])
        case "companion_ask_user_submit_request":
            let payload = try request.body.decode(CompanionAskUserSubmitRequest.self)
            guard let service = companionAskUserService else {
                throw NativeCompanionRelayError.localAgentRuntimeUnavailable
            }
            let updated = try await service.submit(
                promptID: payload.promptID,
                sessionID: payload.conversationID,
                submission: .init(
                    values: payload.values,
                    selection: Self.askUserSelection(payload.selection)
                )
            )
            body = .object([
                "success": .bool(true),
                "prompt": Self.companionAskUserPrompt(updated),
            ])
        case "companion_ask_user_cancel_request":
            let payload = try request.body.decode(CompanionAskUserMutationRequest.self)
            guard let service = companionAskUserService else {
                throw NativeCompanionRelayError.localAgentRuntimeUnavailable
            }
            let updated = try await service.cancel(
                promptID: payload.promptID,
                sessionID: payload.conversationID
            )
            body = .object([
                "success": .bool(true),
                "prompt": Self.companionAskUserPrompt(updated),
            ])
        case "companion_agent_workspace_request":
            let (_, store) = try await companionAgentStore()
            body = try Self.nativeJSON(try await companionAgentWorkspace(
                ownerUserID: ownerUserID,
                store: store
            ))
        case "companion_agent_conversation_request":
            let payload = try request.body.decode(CompanionAgentConversationRequest.self)
            let (_, store) = try await companionAgentStore()
            body = try Self.nativeJSON(try await companionAgentConversationDetail(
                ownerUserID: ownerUserID,
                roomID: payload.roomID,
                store: store
            ))
        case "companion_agent_messages_request":
            let payload = try request.body.decode(CompanionAgentMessagesRequest.self)
            let (_, store) = try await companionAgentStore()
            body = try Self.nativeJSON(try await companionAgentMessages(
                ownerUserID: ownerUserID,
                request: payload,
                store: store
            ))
        case "companion_agent_send_message_request":
            let payload = try request.body.decode(CompanionAgentSendMessageRequest.self)
            let (service, store) = try await companionAgentStore()
            let roomID = payload.roomID.trimmingCharacters(in: .whitespacesAndNewlines)
            let content = payload.content.trimmingCharacters(in: .whitespacesAndNewlines)
            let clientMessageID = payload.clientMessageID
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard !roomID.isEmpty else { throw NativeCompanionRelayError.missingRoomID }
            guard !content.isEmpty else { throw NativeCompanionRelayError.missingMessageContent }
            guard !clientMessageID.isEmpty, clientMessageID.count <= 128 else {
                throw NativeCompanionRelayError.invalidClientMessageID
            }
            guard let room = try await store.room(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active else {
                throw NativeCompanionRelayError.agentResourceNotFound
            }
            guard room.conversationKind != .agentAgentDirect else {
                throw NativeCompanionRelayError.agentConversationReadOnly
            }
            let posted = try await store.postMessageIdempotently(
                ownerUserID: ownerUserID,
                roomID: roomID,
                draft: .init(
                    senderKind: .human,
                    senderID: ownerUserID,
                    content: content,
                    mentionedAgentIDs: payload.mentionedAgentIDs,
                    causationID: "companion:\(clientMessageID)"
                )
            )
            if !posted.deduplicated {
                await service.publishChange(.init(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    kind: .roomUpdated
                ))
                startCompanionAgentScheduler(ownerUserID: ownerUserID)
            }
            body = try Self.nativeJSON(LocalConnectorCompanionAgentSendResponse(
                message: Self.companionAgentMessage(posted.message),
                deduplicated: posted.deduplicated
            ))
        case "companion_agent_open_direct_request":
            let payload = try request.body.decode(CompanionAgentOpenDirectRequest.self)
            let agentID = payload.agentID.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !agentID.isEmpty else { throw NativeCompanionRelayError.missingAgentID }
            let (service, store) = try await companionAgentStore()
            let room = try await store.openHumanAgentDirect(
                ownerUserID: ownerUserID,
                agentID: agentID
            )
            await service.publishChange(.init(
                ownerUserID: ownerUserID,
                roomID: room.id,
                kind: .roomUpdated
            ))
            body = try Self.nativeJSON(try await companionAgentConversationDetail(
                ownerUserID: ownerUserID,
                roomID: room.id,
                store: store
            ))
        case "companion_approvals_request":
            let approvals = try await fetchPendingApprovals().map(Self.companionApproval)
            body = try Self.nativeJSON(approvals)
        case "companion_resolve_approval_request":
            let payload = try request.body.decode(CompanionApprovalDecisionRequest.self)
            let id = payload.approvalID.trimmingCharacters(in: .whitespacesAndNewlines)
            let decision = payload.decision.trimmingCharacters(in: .whitespacesAndNewlines)
            _ = try Self.validateCompanionApprovalResolution(
                id: id,
                decision: decision,
                pending: try await fetchPendingApprovals()
            )
            try await resolveApproval(id: id, decision: decision)
            body = .object(["success": .bool(true), "approval_id": .string(id)])
        default:
            throw NativeCompanionRelayError.unsupportedRequest
        }
        return NativeRelayResponse(
            type: Self.companionResponseType(for: request.type),
            requestID: request.requestID,
            status: 200,
            body: body
        )
    }

    nonisolated private static func nativeJSON<T: Encodable>(_ value: T) throws -> NativeJSONValue {
        try JSONDecoder().decode(NativeJSONValue.self, from: JSONEncoder().encode(value))
    }

    nonisolated static func companionApproval(
        _ approval: LocalConnectorPendingApproval
    ) -> LocalConnectorCompanionApproval {
        let context = URL(fileURLWithPath: approval.cwd).lastPathComponent
        return LocalConnectorCompanionApproval(
            id: approval.id,
            command: approval.command,
            context: context.isEmpty ? nil : context,
            source: approval.source,
            risk: approval.risk,
            reason: approval.reason,
            createdAt: approval.createdAt,
            availableDecisions: approval.availableDecisions.filter {
                Self.isCompanionApprovalDecision($0)
            }
        )
    }

    nonisolated static func isCompanionApprovalDecision(_ decision: String) -> Bool {
        ["accept", "acceptForSession", "decline"].contains(decision)
    }

    nonisolated static func validateCompanionApprovalResolution(
        id: String,
        decision: String,
        pending: [LocalConnectorPendingApproval]
    ) throws -> LocalConnectorPendingApproval {
        guard !id.isEmpty else { throw NativeCompanionRelayError.missingApprovalID }
        guard Self.isCompanionApprovalDecision(decision) else {
            throw NativeCompanionRelayError.invalidApprovalDecision
        }
        guard let approval = pending.first(where: { $0.id == id }) else {
            throw NativeCompanionRelayError.approvalNotFound
        }
        guard approval.availableDecisions.contains(decision) else {
            throw NativeCompanionRelayError.invalidApprovalDecision
        }
        return approval
    }

    nonisolated private static func companionResponseType(for requestType: String?) -> String {
        switch requestType {
        case "companion_resources_request": "companion_resources_response"
        case "companion_resolve_resource_request": "companion_resolve_resource_response"
        case "companion_conversation_request": "companion_conversation_response"
        case "companion_conversation_history_request": "companion_conversation_history_response"
        case "companion_conversation_state_request": "companion_conversation_state_response"
        case "companion_conversation_send_request": "companion_conversation_send_response"
        case "companion_conversation_guidance_request": "companion_conversation_guidance_response"
        case "companion_conversation_stop_request": "companion_conversation_stop_response"
        case "companion_message_tasks_request": "companion_message_tasks_response"
        case "companion_ask_user_prompts_request": "companion_ask_user_prompts_response"
        case "companion_ask_user_submit_request": "companion_ask_user_submit_response"
        case "companion_ask_user_cancel_request": "companion_ask_user_cancel_response"
        case "companion_agent_workspace_request": "companion_agent_workspace_response"
        case "companion_agent_conversation_request": "companion_agent_conversation_response"
        case "companion_agent_messages_request": "companion_agent_messages_response"
        case "companion_agent_send_message_request": "companion_agent_send_message_response"
        case "companion_agent_open_direct_request": "companion_agent_open_direct_response"
        case "companion_approvals_request": "companion_approvals_response"
        case "companion_resolve_approval_request": "companion_resolve_approval_response"
        default: "companion_error_response"
        }
    }

}

private struct CompanionResolveRequest: Decodable {
    var resourceID: String

    enum CodingKeys: String, CodingKey {
        case resourceID = "resource_id"
    }
}

struct CompanionConversationRequest: Decodable {
    var conversationID: String

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
    }
}

struct CompanionConversationHistoryRequest: Decodable {
    var conversationID: String
    var beforeOrdinal: UInt64?
    var limit: Int?

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case beforeOrdinal = "before_ordinal"
        case limit
    }
}

struct CompanionConversationTurnRequest: Decodable {
    var conversationID: String
    var turnID: String
    var content: String

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case turnID = "turn_id"
        case content
    }
}

private struct CompanionConversationStopRequest: Decodable {
    var conversationID: String
    var turnID: String?

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case turnID = "turn_id"
    }
}

struct CompanionMessageTasksRequest: Decodable {
    var conversationID: String
    var messageID: String
    var taskID: String?

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case messageID = "message_id"
        case taskID = "task_id"
    }
}

private struct CompanionAskUserPromptsRequest: Decodable {
    var conversationID: String
    var limit: Int?

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case limit
    }
}

private struct CompanionAskUserMutationRequest: Decodable {
    var conversationID: String
    var promptID: String

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case promptID = "prompt_id"
    }
}

private struct CompanionAskUserSubmitRequest: Decodable {
    var conversationID: String
    var promptID: String
    var values: [String: String]
    var selection: CompanionAskUserSelection?

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case promptID = "prompt_id"
        case values, selection
    }
}

enum CompanionAskUserSelection: Decodable {
    case single(String)
    case multiple([String])

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let value = try? container.decode(String.self) {
            self = .single(value)
        } else {
            self = .multiple(try container.decode([String].self))
        }
    }
}

private struct CompanionApprovalDecisionRequest: Decodable {
    var approvalID: String
    var decision: String

    enum CodingKeys: String, CodingKey {
        case approvalID = "approval_id"
        case decision
    }
}

enum NativeCompanionRelayError: LocalizedError {
    case invalidContext
    case runtimeUnavailable
    case missingResourceID
    case missingApprovalID
    case invalidApprovalDecision
    case approvalNotFound
    case agentRuntimeUnavailable
    case missingRoomID
    case missingAgentID
    case missingMessageContent
    case invalidClientMessageID
    case invalidMessageCursor
    case agentResourceNotFound
    case agentConversationReadOnly
    case localAgentRuntimeUnavailable
    case conversationResourceNotFound
    case unsupportedRequest

    var status: Int {
        switch self {
        case .runtimeUnavailable, .agentRuntimeUnavailable, .localAgentRuntimeUnavailable: 503
        case .invalidContext: 401
        case .approvalNotFound, .agentResourceNotFound, .conversationResourceNotFound: 404
        case .agentConversationReadOnly: 403
        case .missingResourceID, .missingApprovalID, .invalidApprovalDecision,
             .missingRoomID, .missingAgentID, .missingMessageContent,
             .invalidClientMessageID, .invalidMessageCursor, .unsupportedRequest: 400
        }
    }

    var errorDescription: String? {
        switch self {
        case .invalidContext: "本机 Companion 上下文无效。"
        case .runtimeUnavailable: "桌面客户端会话目录尚未就绪。"
        case .missingResourceID: "resource_id 不能为空。"
        case .missingApprovalID: "approval_id 不能为空。"
        case .invalidApprovalDecision: "审批决定无效。"
        case .approvalNotFound: "这个审批请求已经处理或不存在。"
        case .agentRuntimeUnavailable: "桌面客户端 Agent 团队尚未就绪。"
        case .missingRoomID: "room_id 不能为空。"
        case .missingAgentID: "agent_id 不能为空。"
        case .missingMessageContent: "消息内容不能为空。"
        case .invalidClientMessageID: "client_message_id 无效。"
        case .invalidMessageCursor: "消息游标参数无效。"
        case .agentResourceNotFound: "Agent 团队资源不存在。"
        case .agentConversationReadOnly: "Agent 之间的协作私聊只能查看。"
        case .localAgentRuntimeUnavailable: "本地 Agent Host 尚未就绪。"
        case .conversationResourceNotFound: "本地会话资源不存在。"
        case .unsupportedRequest: "不支持的 Companion 请求。"
        }
    }
}
