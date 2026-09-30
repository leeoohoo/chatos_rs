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

    private func companionConversation(
        _ request: CompanionConversationRequest,
        ownerUserID: String
    ) async throws -> NativeJSONValue {
        let detail = try await requireCompanionConversationClient().get(
            ownerUserID: ownerUserID,
            conversationID: request.conversationID
        )
        let active = detail.turns.last(where: { Self.isActiveConversationTurn($0.status) })
        return .object([
            "id": .string(detail.conversation.conversationID),
            "title": .string(detail.conversation.title),
            "status": .string(active?.status ?? "idle"),
            "message_count": .number(Double(detail.messages.count)),
            "updated_at": .string(Self.iso8601(detail.conversation.updatedAtUnixMs)),
        ])
    }

    private func companionConversationHistory(
        _ request: CompanionConversationHistoryRequest,
        ownerUserID: String
    ) async throws -> NativeJSONValue {
        let page = try await requireCompanionConversationClient().history(
            ownerUserID: ownerUserID,
            conversationID: request.conversationID,
            beforeOrdinal: request.beforeOrdinal,
            limit: UInt32(min(100, max(1, request.limit ?? 40)))
        )
        let items = page.messages.sorted { $0.ordinal < $1.ordinal }.map { message in
            var value: [String: NativeJSONValue] = [
                "id": .string(message.messageID),
                "role": .string(message.role),
                "content": .string(Self.conversationText(message.content)),
                "revision": .number(Double(page.conversation.version)),
                "sequence_no": .number(Double(message.ordinal)),
                "created_at": .string(Self.iso8601(message.createdAtUnixMs)),
            ]
            if let source = message.metadata.stringValue(for: "source") {
                value["message_source"] = .string(source)
            }
            if let taskID = message.metadata.stringValue(for: "task_id") {
                value["message_mode"] = .string("local_task_callback")
                value["task_id"] = .string(taskID)
            }
            return NativeJSONValue.object(value)
        }
        var response: [String: NativeJSONValue] = [
            "items": .array(items),
            "has_more": .bool(page.nextBeforeOrdinal != nil),
            "snapshot_revision": .number(Double(page.conversation.version)),
        ]
        if let next = page.nextBeforeOrdinal {
            response["next_before"] = .string(String(next))
        }
        return .object(response)
    }

    private func companionConversationState(
        _ request: CompanionConversationRequest,
        ownerUserID: String
    ) async throws -> NativeJSONValue {
        let detail = try await requireCompanionConversationClient().get(
            ownerUserID: ownerUserID,
            conversationID: request.conversationID
        )
        guard let active = detail.turns.last(where: {
            Self.isActiveConversationTurn($0.status)
        }) else { return .null }
        return .object([
            "turn_id": .string(active.turnID),
            "conversation_turn_id": .string(active.turnID),
            "status": .string(active.status),
            "active_in_runtime": .bool(true),
        ])
    }

    private func sendCompanionConversationTurn(
        _ request: CompanionConversationTurnRequest,
        guidance: Bool
    ) async throws -> NativeJSONValue {
        guard let service = companionConversationService else {
            throw NativeCompanionRelayError.localAgentRuntimeUnavailable
        }
        let command = ConversationSendCommand(
            sessionID: request.conversationID,
            turnID: request.turnID,
            content: request.content
        )
        let acknowledgment = try await (guidance
            ? service.sendGuidance(command)
            : service.sendNewTurn(command))
        var response: [String: NativeJSONValue] = [
            "accepted": .bool(acknowledgment.accepted),
            "conversation_id": .string(request.conversationID),
            "turn_id": .string(acknowledgment.turnID),
        ]
        if let messageID = acknowledgment.userMessageID {
            response["user_message_id"] = .string(messageID)
            response["message_id"] = .string(messageID)
        }
        return .object(response)
    }

    private func companionMessageTasks(
        _ request: CompanionMessageTasksRequest,
        ownerUserID: String
    ) async throws -> NativeJSONValue {
        guard let service = companionMessageTaskService else {
            throw NativeCompanionRelayError.localAgentRuntimeUnavailable
        }
        let client = try requireCompanionConversationClient()
        var before: UInt64?
        var turnID: String?
        for _ in 0..<20 {
            let page = try await client.history(
                ownerUserID: ownerUserID,
                conversationID: request.conversationID,
                beforeOrdinal: before,
                limit: 100
            )
            if let message = page.messages.first(where: { $0.messageID == request.messageID }) {
                turnID = message.turnID
                break
            }
            guard let next = page.nextBeforeOrdinal, next != before else { break }
            before = next
        }
        guard let turnID else { throw NativeCompanionRelayError.conversationResourceNotFound }
        let lookup = MessageTaskLookup(
            sessionID: request.conversationID,
            turnID: turnID,
            sourceUserMessageID: request.messageID
        )
        let graph = try await service.fetchGraph(messageID: request.messageID, lookup: lookup)
        let selected = graph.nodes.map(\.task).filter {
            request.taskID == nil || $0.id == request.taskID
        }
        var tasks: [NativeJSONValue] = []
        for task in selected {
            let detail = (try? await service.fetchTask(
                messageID: request.messageID,
                taskID: task.id,
                lookup: lookup
            )) ?? task
            tasks.append(Self.companionTask(detail))
        }
        return .object(["items": .array(tasks)])
    }

    private func requireCompanionConversationClient() throws -> NativeLocalAgentConversationClient {
        guard let companionConversationClient else {
            throw NativeCompanionRelayError.localAgentRuntimeUnavailable
        }
        return companionConversationClient
    }

    nonisolated private static func companionTask(_ task: MessageTask) -> NativeJSONValue {
        var value: [String: NativeJSONValue] = [
            "id": .string(task.id),
            "title": .string(task.title),
            "tags": .array(task.tags.map(NativeJSONValue.string)),
        ]
        value.set("description", task.description)
        value.set("objective", task.objective)
        value.set("status", task.status)
        value.set("result_summary", task.resultSummary)
        value.set("process_log", task.processLog)
        value.set("created_at", task.createdAt.map(iso8601))
        value.set("updated_at", task.updatedAt.map(iso8601))
        if let priority = task.priority { value["priority"] = .number(Double(priority)) }
        if let run = task.lastRun {
            var lastRun: [String: NativeJSONValue] = ["id": .string(run.id)]
            lastRun.set("status", run.status)
            lastRun.set("model_phase_status", run.modelPhaseStatus)
            lastRun.set("result_summary", run.resultSummary)
            lastRun.set("error_message", run.errorMessage)
            lastRun.set("started_at", run.startedAt.map(iso8601))
            lastRun.set("finished_at", run.finishedAt.map(iso8601))
            if let report = run.reportContent {
                lastRun["report"] = .object(["content": .string(report)])
            }
            value["last_run"] = .object(lastRun)
        }
        return .object(value)
    }

    nonisolated private static func companionAskUserPrompt(
        _ prompt: AskUserPrompt
    ) -> NativeJSONValue {
        let fields = prompt.fields.map { field in
            NativeJSONValue.object([
                "key": .string(field.key),
                "label": .string(field.label),
                "description": field.description.map(NativeJSONValue.string) ?? .null,
                "placeholder": field.placeholder.map(NativeJSONValue.string) ?? .null,
                "default_value": .string(field.defaultValue),
                "required": .bool(field.isRequired),
                "multiline": .bool(field.isMultiline),
                "secret": .bool(field.isSecret),
            ])
        }
        let choice: NativeJSONValue = prompt.choice.map { choice in
            .object([
                "multiple": .bool(choice.allowsMultiple),
                "options": .array(choice.options.map { option in
                    .object([
                        "value": .string(option.value),
                        "label": .string(option.label),
                        "description": option.description.map(NativeJSONValue.string) ?? .null,
                    ])
                }),
                "default": .array(choice.defaultSelection.map(NativeJSONValue.string)),
                "min_selections": .number(Double(choice.minimumSelectionCount)),
                "max_selections": .number(Double(choice.maximumSelectionCount)),
            ])
        } ?? .null
        let timestamp = iso8601(prompt.updatedAt ?? prompt.createdAt ?? Date())
        return .object([
            "id": .string(prompt.id),
            "conversation_id": .string(prompt.sessionID),
            "conversation_turn_id": .string(prompt.turnID),
            "kind": .string(prompt.kind),
            "status": .string(prompt.status.rawValue),
            "prompt": .object([
                "title": .string(prompt.title),
                "message": .string(prompt.message),
                "allow_cancel": .bool(prompt.allowsCancel),
                "payload": .object(["fields": .array(fields), "choice": choice]),
            ]),
            "created_at": .string(iso8601(prompt.createdAt ?? Date())),
            "updated_at": .string(timestamp),
        ])
    }

    nonisolated private static func askUserSelection(
        _ selection: CompanionAskUserSelection?
    ) -> AskUserSelection? {
        switch selection {
        case let .single(value): .single(value)
        case let .multiple(values): .multiple(values)
        case nil: nil
        }
    }

    nonisolated private static func isActiveConversationTurn(_ status: String) -> Bool {
        !["succeeded", "failed", "cancelled"].contains(status)
    }

    nonisolated private static func conversationText(_ value: LocalAgentJSONValue) -> String {
        switch value {
        case .null: ""
        case let .bool(value): String(value)
        case let .number(value): String(value)
        case let .string(value): value
        case let .array(values): values.map(conversationText).joined(separator: "\n")
        case let .object(values):
            values["text"].map(conversationText)
                ?? values["content"].map(conversationText)
                ?? ""
        }
    }

    nonisolated private static func iso8601(_ unixMilliseconds: Int64) -> String {
        iso8601(Date(timeIntervalSince1970: Double(unixMilliseconds) / 1_000))
    }

    nonisolated private static func iso8601(_ date: Date) -> String {
        ISO8601DateFormatter().string(from: date)
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

private struct CompanionConversationRequest: Decodable {
    var conversationID: String

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
    }
}

private struct CompanionConversationHistoryRequest: Decodable {
    var conversationID: String
    var beforeOrdinal: UInt64?
    var limit: Int?

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case beforeOrdinal = "before_ordinal"
        case limit
    }
}

private struct CompanionConversationTurnRequest: Decodable {
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

private struct CompanionMessageTasksRequest: Decodable {
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

private enum CompanionAskUserSelection: Decodable {
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

private extension Dictionary where Key == String, Value == NativeJSONValue {
    mutating func set(_ key: String, _ value: String?) {
        if let value { self[key] = .string(value) }
    }
}

private extension LocalAgentJSONValue {
    func stringValue(for key: String) -> String? {
        guard case let .object(values) = self,
              case let .string(value)? = values[key],
              !value.isEmpty else { return nil }
        return value
    }
}
