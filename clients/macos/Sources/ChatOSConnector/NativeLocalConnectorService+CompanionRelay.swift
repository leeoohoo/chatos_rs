import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    nonisolated static func isCompanionRelayMessageType(_ messageType: String) -> Bool {
        switch messageType {
        case "companion_resources_request",
             "companion_resolve_resource_request",
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

    private func companionAgentStore() async throws -> (
        NativeAgentGroupChatService,
        SQLiteAgentGroupChatStore
    ) {
        guard let service = agentGroupChatService else {
            throw NativeCompanionRelayError.agentRuntimeUnavailable
        }
        do {
            return (service, try await service.store())
        } catch {
            throw NativeCompanionRelayError.agentRuntimeUnavailable
        }
    }

    private func companionAgentWorkspace(
        ownerUserID: String,
        store: SQLiteAgentGroupChatStore
    ) async throws -> LocalConnectorCompanionAgentWorkspace {
        let profiles = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: true)
        let activeAgents = profiles.filter { $0.status == .active }.map(Self.companionAgentSummary)
        let teams = try await store.listRooms(ownerUserID: ownerUserID, includeArchived: false)
        let directs = try await store.listDirectConversations(
            ownerUserID: ownerUserID,
            includeArchived: false
        )
        var teamSummaries: [LocalConnectorCompanionAgentConversationSummary] = []
        for room in teams {
            teamSummaries.append(try await companionAgentConversationSummary(
                ownerUserID: ownerUserID,
                room: room,
                store: store
            ))
        }
        var directSummaries: [LocalConnectorCompanionAgentConversationSummary] = []
        for room in directs {
            directSummaries.append(try await companionAgentConversationSummary(
                ownerUserID: ownerUserID,
                room: room,
                store: store
            ))
        }
        return .init(
            teams: teamSummaries.sorted { $0.updatedAtUnixMs > $1.updatedAtUnixMs },
            directConversations: directSummaries.sorted {
                $0.updatedAtUnixMs > $1.updatedAtUnixMs
            },
            agents: activeAgents
        )
    }

    private func companionAgentConversationSummary(
        ownerUserID: String,
        room: ProjectAgentRoom,
        store: SQLiteAgentGroupChatStore
    ) async throws -> LocalConnectorCompanionAgentConversationSummary {
        let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
        let recent = try await store.pageRecentMessages(
            ownerUserID: ownerUserID,
            roomID: room.id,
            beforeMessageID: nil,
            limit: 1
        )
        return .init(
            id: room.id,
            kind: room.conversationKind.rawValue,
            title: room.draft.name,
            goal: room.draft.goal,
            projectID: room.projectID,
            defaultAgentID: room.defaultAgentID,
            memberCount: members.count,
            canSend: room.conversationKind != .agentAgentDirect,
            updatedAtUnixMs: max(room.updatedAtUnixMs, recent.messages.last?.createdAtUnixMs ?? 0),
            lastMessage: recent.messages.last.map(Self.companionAgentMessage)
        )
    }

    private func companionAgentConversationDetail(
        ownerUserID: String,
        roomID: String,
        store: SQLiteAgentGroupChatStore
    ) async throws -> LocalConnectorCompanionAgentConversationDetail {
        let normalizedRoomID = roomID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedRoomID.isEmpty else { throw NativeCompanionRelayError.missingRoomID }
        guard let room = try await store.room(
            ownerUserID: ownerUserID,
            roomID: normalizedRoomID
        ), room.status == .active else {
            throw NativeCompanionRelayError.agentResourceNotFound
        }
        let profiles = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: true)
        let profilesByID = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0) })
        let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
            .compactMap { member -> LocalConnectorCompanionAgentMemberSummary? in
                guard let profile = profilesByID[member.agentID] else { return nil }
                return .init(
                    agent: Self.companionAgentSummary(profile),
                    role: member.draft.role,
                    responsibility: member.draft.responsibility
                )
            }
        return .init(
            conversation: try await companionAgentConversationSummary(
                ownerUserID: ownerUserID,
                room: room,
                store: store
            ),
            members: members
        )
    }

    private func companionAgentMessages(
        ownerUserID: String,
        request: CompanionAgentMessagesRequest,
        store: SQLiteAgentGroupChatStore
    ) async throws -> LocalConnectorCompanionAgentMessagePage {
        let roomID = request.roomID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !roomID.isEmpty else { throw NativeCompanionRelayError.missingRoomID }
        guard request.beforeMessageID == nil || request.afterMessageID == nil else {
            throw NativeCompanionRelayError.invalidMessageCursor
        }
        let limit = min(100, max(1, request.limit ?? 40))
        let page: ProjectAgentMessagePage
        if let afterMessageID = request.afterMessageID {
            page = try await store.pageMessages(
                ownerUserID: ownerUserID,
                roomID: roomID,
                afterMessageID: afterMessageID,
                limit: limit
            )
        } else {
            page = try await store.pageRecentMessages(
                ownerUserID: ownerUserID,
                roomID: roomID,
                beforeMessageID: request.beforeMessageID,
                limit: limit
            )
        }
        return .init(
            messages: page.messages.map(Self.companionAgentMessage),
            nextCursorMessageID: page.nextCursorMessageID,
            hasMore: page.hasMore
        )
    }

    nonisolated private static func companionAgentSummary(
        _ profile: LocalAgentProfile
    ) -> LocalConnectorCompanionAgentSummary {
        .init(
            id: profile.id,
            name: profile.draft.name,
            description: profile.draft.description,
            professionKey: profile.draft.professionKey,
            status: profile.status.rawValue,
            heartbeatEnabled: profile.draft.heartbeatEnabled,
            lastHeartbeatAtUnixMs: profile.lastHeartbeatAtUnixMs,
            updatedAtUnixMs: profile.updatedAtUnixMs
        )
    }

    nonisolated private static func companionAgentMessage(
        _ message: ProjectAgentMessage
    ) -> LocalConnectorCompanionAgentMessage {
        .init(
            id: message.id,
            roomID: message.roomID,
            senderKind: message.senderKind.rawValue,
            senderID: message.senderID,
            content: message.content,
            mentionedAgentIDs: message.mentionedAgentIDs,
            replyToMessageID: message.replyToMessageID,
            createdAtUnixMs: message.createdAtUnixMs,
            attachments: message.attachmentItems.map {
                .init(
                    id: $0.id,
                    name: $0.name,
                    mimeType: $0.mimeType,
                    size: $0.size,
                    kind: $0.kind.rawValue
                )
            }
        )
    }

    private func startCompanionAgentScheduler(ownerUserID: String) {
        guard let scheduler = agentGroupChatScheduler else { return }
        Task {
            _ = try? await scheduler.drainAccount(ownerUserID: ownerUserID)
        }
    }
}

private struct CompanionResolveRequest: Decodable {
    var resourceID: String

    enum CodingKeys: String, CodingKey {
        case resourceID = "resource_id"
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

private struct CompanionAgentConversationRequest: Decodable {
    var roomID: String

    enum CodingKeys: String, CodingKey {
        case roomID = "room_id"
    }
}

private struct CompanionAgentMessagesRequest: Decodable {
    var roomID: String
    var beforeMessageID: String?
    var afterMessageID: String?
    var limit: Int?

    enum CodingKeys: String, CodingKey {
        case roomID = "room_id"
        case beforeMessageID = "before_message_id"
        case afterMessageID = "after_message_id"
        case limit
    }
}

private struct CompanionAgentSendMessageRequest: Decodable {
    var roomID: String
    var content: String
    var mentionedAgentIDs: [String]
    var clientMessageID: String

    enum CodingKeys: String, CodingKey {
        case roomID = "room_id"
        case content
        case mentionedAgentIDs = "mentioned_agent_ids"
        case clientMessageID = "client_message_id"
    }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        roomID = try values.decode(String.self, forKey: .roomID)
        content = try values.decode(String.self, forKey: .content)
        mentionedAgentIDs = try values.decodeIfPresent(
            [String].self,
            forKey: .mentionedAgentIDs
        ) ?? []
        clientMessageID = try values.decode(String.self, forKey: .clientMessageID)
    }
}

private struct CompanionAgentOpenDirectRequest: Decodable {
    var agentID: String

    enum CodingKeys: String, CodingKey {
        case agentID = "agent_id"
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
    case unsupportedRequest

    var status: Int {
        switch self {
        case .runtimeUnavailable, .agentRuntimeUnavailable: 503
        case .invalidContext: 401
        case .approvalNotFound, .agentResourceNotFound: 404
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
        case .unsupportedRequest: "不支持的 Companion 请求。"
        }
    }
}
