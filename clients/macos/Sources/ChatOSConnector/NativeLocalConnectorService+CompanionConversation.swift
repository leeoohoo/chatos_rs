import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    func companionConversation(
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

    func companionConversationHistory(
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

    func companionConversationState(
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

    func sendCompanionConversationTurn(
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

    func companionMessageTasks(
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

    func requireCompanionConversationClient() throws -> NativeLocalAgentConversationClient {
        guard let companionConversationClient else {
            throw NativeCompanionRelayError.localAgentRuntimeUnavailable
        }
        return companionConversationClient
    }

    nonisolated static func companionTask(_ task: MessageTask) -> NativeJSONValue {
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

    nonisolated static func companionAskUserPrompt(
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

    nonisolated static func askUserSelection(
        _ selection: CompanionAskUserSelection?
    ) -> AskUserSelection? {
        switch selection {
        case let .single(value): .single(value)
        case let .multiple(values): .multiple(values)
        case nil: nil
        }
    }

    nonisolated static func isActiveConversationTurn(_ status: String) -> Bool {
        !["succeeded", "failed", "cancelled"].contains(status)
    }

    nonisolated static func conversationText(_ value: LocalAgentJSONValue) -> String {
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

    nonisolated static func iso8601(_ unixMilliseconds: Int64) -> String {
        iso8601(Date(timeIntervalSince1970: Double(unixMilliseconds) / 1_000))
    }

    nonisolated static func iso8601(_ date: Date) -> String {
        ISO8601DateFormatter().string(from: date)
    }
}

extension Dictionary where Key == String, Value == NativeJSONValue {
    mutating func set(_ key: String, _ value: String?) {
        if let value { self[key] = .string(value) }
    }
}

extension LocalAgentJSONValue {
    func stringValue(for key: String) -> String? {
        guard case let .object(values) = self,
              case let .string(value)? = values[key],
              !value.isEmpty else { return nil }
        return value
    }
}
