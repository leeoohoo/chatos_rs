import Foundation

public actor ConversationHistoryStore {
    private struct SessionState: Sendable {
        var turnsByID: [String: ConversationTurn] = [:]
        var olderCursor: String?
        var hasOlder = false
        var snapshotRevision: Int64 = 0
        var newestAcceptedLatestGeneration: Int64 = 0
        var newestAcceptedOlderGeneration: Int64 = 0
        var hasLoadedOlderPage = false
        var lastAppliedEventSequence: Int64 = 0
        var appliedEventIDs: Set<String> = []
        var lastAppliedLocalAgentEventSequence: UInt64 = 0
        var localAgentPromptsByID: [String: AskUserPrompt] = [:]
        var localAgentPromptRoutes: [String: LocalAgentAskUserRoute] = [:]
        var viewportAnchor: ViewportAnchor?
        var unreadNewerCount = 0
    }

    private var sessions: [String: SessionState] = [:]
    private var localUpdateContinuations: [
        String: [UUID: AsyncStream<Void>.Continuation]
    ] = [:]

    public init() {}

    public func mergeCachedTurns(_ turns: [ConversationTurn], sessionID: String) {
        var state = sessions[sessionID] ?? SessionState()
        merge(
            turns,
            sessionID: sessionID,
            replacingChangedEqualRevisions: false,
            into: &state
        )
        sessions[sessionID] = state
    }

    public func mergePage(
        _ page: HistoryPage,
        sessionID: String,
        origin: ConversationHistoryPageOrigin = .latest
    ) {
        var state = sessions[sessionID] ?? SessionState()
        let acceptsLatestSnapshot = origin == .latest
            && page.requestGeneration >= state.newestAcceptedLatestGeneration
        let didChange = merge(
            page.turns,
            sessionID: sessionID,
            replacingChangedEqualRevisions: acceptsLatestSnapshot,
            into: &state
        )

        if didChange,
           origin == .latest,
           state.viewportAnchor?.isPinnedToBottom == false {
            state.unreadNewerCount += 1
        }

        switch origin {
        case .latest:
            if page.requestGeneration >= state.newestAcceptedLatestGeneration {
                state.newestAcceptedLatestGeneration = page.requestGeneration
                if !state.hasLoadedOlderPage {
                    state.olderCursor = page.olderCursor
                    state.hasOlder = page.hasOlder
                }
            }
        case .older:
            if page.requestGeneration >= state.newestAcceptedOlderGeneration {
                state.newestAcceptedOlderGeneration = page.requestGeneration
                state.hasLoadedOlderPage = true
                state.olderCursor = page.olderCursor
                state.hasOlder = page.hasOlder
            }
        }
        state.snapshotRevision = max(state.snapshotRevision, page.snapshotRevision)

        sessions[sessionID] = state
    }

    public func applyRealtime(_ event: RealtimeTurnEvent, userIsReadingOlderContent: Bool) {
        let sessionID = event.turn.sessionID
        var state = sessions[sessionID] ?? SessionState()

        guard !state.appliedEventIDs.contains(event.eventID) else {
            return
        }

        state.appliedEventIDs.insert(event.eventID)
        state.lastAppliedEventSequence = max(state.lastAppliedEventSequence, event.eventSequence)
        let didChange = merge(
            [event.turn],
            sessionID: sessionID,
            replacingChangedEqualRevisions: false,
            into: &state
        )

        if didChange, userIsReadingOlderContent {
            state.unreadNewerCount += 1
        }

        sessions[sessionID] = state
    }

    public func setViewportAnchor(_ anchor: ViewportAnchor?, sessionID: String) {
        var state = sessions[sessionID] ?? SessionState()
        state.viewportAnchor = anchor
        if anchor?.isPinnedToBottom == true {
            state.unreadNewerCount = 0
        }
        sessions[sessionID] = state
    }

    public func markNewerContentRead(sessionID: String) {
        var state = sessions[sessionID] ?? SessionState()
        state.unreadNewerCount = 0
        sessions[sessionID] = state
    }

    public func discardOptimisticTurn(sessionID: String, turnID: String) {
        var state = sessions[sessionID] ?? SessionState()
        guard state.turnsByID[turnID]?.revision == 0 else { return }
        state.turnsByID.removeValue(forKey: turnID)
        sessions[sessionID] = state
    }

    public func snapshot(sessionID: String) -> ConversationHistorySnapshot {
        let state = sessions[sessionID] ?? SessionState()
        return ConversationHistorySnapshot(
            sessionID: sessionID,
            turns: state.turnsByID.values.sorted(by: ConversationTurn.isOrderedBefore),
            olderCursor: state.olderCursor,
            hasOlder: state.hasOlder,
            snapshotRevision: state.snapshotRevision,
            viewportAnchor: state.viewportAnchor,
            unreadNewerCount: state.unreadNewerCount
        )
    }

    public func localAgentUpdates(sessionID: String) -> AsyncStream<Void> {
        let subscriptionID = UUID()
        return AsyncStream { continuation in
            localUpdateContinuations[sessionID, default: [:]][subscriptionID] = continuation
            continuation.onTermination = { [weak self] _ in
                Task {
                    await self?.removeLocalUpdateContinuation(
                        subscriptionID,
                        sessionID: sessionID
                    )
                }
            }
        }
    }

    public func localAgentPrompts(
        sessionID: String,
        limit: Int
    ) throws -> [AskUserPrompt] {
        guard limit > 0 else { return [] }
        guard let prompts = sessions[sessionID]?.localAgentPromptsByID.values else { return [] }
        return Array(prompts
            .sorted(by: Self.localAgentPromptOrder)
            .suffix(limit))
    }

    public func localAgentPromptRoute(
        promptID: String,
        sessionID: String
    ) throws -> LocalAgentAskUserRoute {
        guard let state = sessions[sessionID],
              state.localAgentPromptsByID[promptID]?.status.isPending == true,
              let route = state.localAgentPromptRoutes[promptID]
        else {
            throw LocalAgentConversationHistoryError.promptUnavailable
        }
        return route
    }

    public func updateLocalAgentPromptStatus(
        promptID: String,
        sessionID: String,
        status: AskUserPromptStatus
    ) throws -> AskUserPrompt {
        guard var state = sessions[sessionID],
              var prompt = state.localAgentPromptsByID[promptID]
        else {
            throw LocalAgentConversationHistoryError.promptUnavailable
        }
        prompt.status = status
        prompt.updatedAt = Date()
        state.localAgentPromptsByID[promptID] = prompt
        if !status.isPending { state.localAgentPromptRoutes[promptID] = nil }
        sessions[sessionID] = state
        localUpdateContinuations[sessionID]?.values.forEach { $0.yield(()) }
        return prompt
    }

    public func applyLocalAgentUIEvent(
        _ event: LocalAgentUIEvent,
        mainChatBinding binding: LocalAgentMainChatRunBinding?
    ) throws {
        guard let binding else { return }
        guard event.event.runID == binding.runID else {
            throw LocalAgentConversationHistoryError.runBindingMismatch
        }

        var state = sessions[binding.threadID] ?? SessionState()
        guard event.eventSeq > state.lastAppliedLocalAgentEventSequence else { return }
        var turn = try localAgentTurn(binding: binding, existing: state.turnsByID[binding.turnID])
        let didChange = try apply(event, to: &turn)
        switch event.event {
        case let .userInteraction(interaction):
            let prompt = Self.askUserPrompt(
                interaction,
                binding: binding,
                emittedAt: event.emittedAt
            )
            state.localAgentPromptsByID[prompt.id] = prompt
            state.localAgentPromptRoutes[prompt.id] = LocalAgentAskUserRoute(
                runID: interaction.runID,
                interactionID: interaction.interactionID
            )
        case let .runSnapshot(run) where run.status == .failed || run.status == .cancelled:
            let resolvedStatus: AskUserPromptStatus = run.status == .cancelled ? .canceled : .failed
            let promptIDs = state.localAgentPromptRoutes.compactMap { id, route in
                route.runID == run.runID ? id : nil
            }
            for id in promptIDs {
                state.localAgentPromptsByID[id]?.status = resolvedStatus
                state.localAgentPromptsByID[id]?.updatedAt = Self.localAgentDate(event.emittedAt)
                state.localAgentPromptRoutes[id] = nil
            }
        default:
            break
        }
        state.lastAppliedLocalAgentEventSequence = event.eventSeq
        if didChange {
            turn.revision = max(turn.revision, Int64(clamping: event.eventSeq))
            state.turnsByID[turn.id] = turn
            if state.viewportAnchor?.isPinnedToBottom == false {
                state.unreadNewerCount += 1
            }
        }
        sessions[binding.threadID] = state
        localUpdateContinuations[binding.threadID]?.values.forEach { $0.yield(()) }
    }

    @discardableResult
    private func merge(
        _ incomingTurns: [ConversationTurn],
        sessionID: String,
        replacingChangedEqualRevisions: Bool,
        into state: inout SessionState
    ) -> Bool {
        var didChange = false

        for incomingTurn in incomingTurns {
            var turn = incomingTurn
            guard turn.sessionID == sessionID else { continue }

            guard let existing = state.turnsByID[turn.id] else {
                state.turnsByID[turn.id] = turn
                didChange = true
                continue
            }

            if turn.revision > existing.revision
                || (replacingChangedEqualRevisions
                    && turn.revision == existing.revision
                    && turn != existing) {
                if !existing.isTaskGraphAvailable {
                    turn.isTaskGraphAvailable = false
                }
                state.turnsByID[turn.id] = turn
                didChange = true
            }
        }

        return didChange
    }

    private func removeLocalUpdateContinuation(_ id: UUID, sessionID: String) {
        localUpdateContinuations[sessionID]?[id] = nil
        if localUpdateContinuations[sessionID]?.isEmpty == true {
            localUpdateContinuations[sessionID] = nil
        }
    }

    private func localAgentTurn(
        binding: LocalAgentMainChatRunBinding,
        existing: ConversationTurn?
    ) throws -> ConversationTurn {
        if let existing {
            guard existing.id == binding.turnID,
                  existing.sessionID == binding.threadID,
                  existing.userMessage.id == binding.messageID
            else {
                throw LocalAgentConversationHistoryError.userMessageBindingMismatch
            }
            return existing
        }

        let message = binding.userMessage
        guard message.recordID == binding.messageID,
              message.runID == binding.runID,
              message.threadID == binding.threadID,
              message.turnID == binding.turnID,
              message.role == .user,
              message.messageMode == .semantic,
              message.messageSource == "main_chat"
        else {
            throw LocalAgentConversationHistoryError.userMessageBindingMismatch
        }
        let createdAt = Self.localAgentDate(message.createdAt) ?? .distantPast
        return ConversationTurn(
            id: binding.turnID,
            sessionID: binding.threadID,
            sequence: Int64(clamping: message.sequence),
            revision: 0,
            userMessage: ChatMessage(
                id: message.recordID,
                role: .user,
                text: message.content ?? "",
                createdAt: createdAt,
                attachments: Self.localAgentAttachments(message.structuredPayload)
            ),
            isTaskGraphAvailable: false,
            status: .queued,
            startedAt: createdAt
        )
    }

    @discardableResult
    private func apply(_ event: LocalAgentUIEvent, to turn: inout ConversationTurn) throws -> Bool {
        switch event.event {
        case let .runSnapshot(run):
            let status = Self.turnStatus(run.status)
            let detail = Self.runDetail(run)
            upsertProcessEvent(
                TurnProcessEvent(
                    id: "local-agent-run-\(run.runID)",
                    title: Self.runTitle(run.status),
                    detail: detail,
                    status: status
                ),
                in: &turn
            )
            turn.status = status
            if let startedAt = Self.localAgentDate(run.createdAt), turn.startedAt == .distantPast {
                turn.startedAt = startedAt
            }
            if run.status == .succeeded {
                guard let text = run.terminalOutcome?.stringValue(forKey: "text")?.nonEmpty else {
                    throw LocalAgentConversationHistoryError.missingSuccessfulOutcome
                }
                setAssistantText(text, runID: run.runID, createdAt: event.emittedAt, in: &turn)
            }
            if status == .completed || status == .failed || status == .cancelled {
                turn.completedAt = Self.localAgentDate(run.updatedAt)
                    ?? Self.localAgentDate(event.emittedAt)
            }
            return true

        case let .modelStream(stream):
            switch stream.deltaKind {
            case .content:
                appendAssistantText(
                    stream.delta,
                    runID: stream.runID,
                    createdAt: event.emittedAt,
                    in: &turn
                )
            case .reasoning:
                appendProcessDetail(
                    stream.delta,
                    id: "local-agent-reasoning-\(stream.runID)-\(stream.stepSeq)",
                    title: "思考过程",
                    in: &turn
                )
            case .status:
                appendProcessDetail(
                    stream.delta,
                    id: "local-agent-model-status-\(stream.runID)-\(stream.stepSeq)",
                    title: "模型状态",
                    in: &turn
                )
            }
            turn.status = .streaming
            return true

        case let .toolSnapshot(tool):
            upsertProcessEvent(
                TurnProcessEvent(
                    id: "local-agent-tool-\(tool.invocationID)",
                    title: tool.toolName,
                    detail: Self.toolDetail(tool),
                    status: Self.toolStatus(tool.status)
                ),
                in: &turn
            )
            if !turn.status.isTerminal { turn.status = .streaming }
            return true

        case let .userInteraction(interaction):
            let options = interaction.options.map(\.label).joined(separator: " / ")
            upsertProcessEvent(
                TurnProcessEvent(
                    id: "local-agent-interaction-\(interaction.interactionID)",
                    title: "等待你的选择",
                    detail: [interaction.prompt, options.nonEmpty].compactMap { $0 }.joined(separator: "\n"),
                    status: .queued
                ),
                in: &turn
            )
            turn.status = .streaming
            return true

        case let .memorySync(status):
            let processStatus: TurnStatus = status.failedCount > 0
                ? .failed
                : (status.pendingCount > 0 ? .queued : .completed)
            upsertProcessEvent(
                TurnProcessEvent(
                    id: "local-agent-memory-\(status.runID ?? "account")",
                    title: "记忆同步",
                    detail: Self.memoryDetail(status),
                    status: processStatus
                ),
                in: &turn
            )
            return true

        case .hostStatus:
            return false
        }
    }

    private func appendAssistantText(
        _ delta: String,
        runID: String,
        createdAt: String,
        in turn: inout ConversationTurn
    ) {
        let messageID = "local-agent-assistant-\(runID)"
        if turn.finalAssistantMessage?.id != messageID {
            turn.finalAssistantMessage = ChatMessage(
                id: messageID,
                role: .assistant,
                text: "",
                createdAt: Self.localAgentDate(createdAt) ?? Date()
            )
        }
        turn.finalAssistantMessage?.text += delta
    }

    private func setAssistantText(
        _ text: String,
        runID: String,
        createdAt: String,
        in turn: inout ConversationTurn
    ) {
        let messageID = "local-agent-assistant-\(runID)"
        if turn.finalAssistantMessage?.id == messageID {
            turn.finalAssistantMessage?.text = text
        } else {
            turn.finalAssistantMessage = ChatMessage(
                id: messageID,
                role: .assistant,
                text: text,
                createdAt: Self.localAgentDate(createdAt) ?? Date()
            )
        }
    }

    private func appendProcessDetail(
        _ delta: String,
        id: String,
        title: String,
        in turn: inout ConversationTurn
    ) {
        if let index = turn.processEvents.firstIndex(where: { $0.id == id }) {
            turn.processEvents[index].detail = (turn.processEvents[index].detail ?? "") + delta
            turn.processEvents[index].status = .streaming
        } else {
            turn.processEvents.append(TurnProcessEvent(
                id: id,
                title: title,
                detail: delta,
                status: .streaming
            ))
        }
    }

    private func upsertProcessEvent(_ event: TurnProcessEvent, in turn: inout ConversationTurn) {
        if let index = turn.processEvents.firstIndex(where: { $0.id == event.id }) {
            turn.processEvents[index] = event
        } else {
            turn.processEvents.append(event)
        }
    }

    private static func localAgentDate(_ value: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: value) { return date }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: value)
    }

    private static func localAgentAttachments(
        _ payload: LocalAgentJSONValue?
    ) -> [ConversationAttachmentReference] {
        guard let attachments = payload?
            .objectValue?["attachments"]?
            .arrayValue
        else { return [] }
        return attachments.enumerated().compactMap { index, value in
            guard let object = value.objectValue,
                  let id = object["attachment_id"]?.plainString,
                  let mediaType = object["media_type"]?.plainString,
                  let byteSize = object["byte_size"]?.integerValue
            else { return nil }
            let kind: ConversationAttachmentKind
            if mediaType.hasPrefix("image/") { kind = .image }
            else if mediaType.hasPrefix("audio/") { kind = .audio }
            else { kind = .file }
            return ConversationAttachmentReference(
                id: id,
                name: "附件 \(index + 1)",
                mimeType: mediaType,
                size: Int(clamping: byteSize),
                kind: kind
            )
        }
    }

    private static func askUserPrompt(
        _ event: LocalAgentUserInteractionEvent,
        binding: LocalAgentMainChatRunBinding,
        emittedAt: String
    ) -> AskUserPrompt {
        let details = event.details?.objectValue
        let title = details?["title"]?.plainString?.nonEmpty ?? "需要你的确认"
        let kind = details?["kind"]?.plainString?.nonEmpty ?? "local_agent"
        let allowsCancel = details?["allows_cancel"]?.boolValue ?? true
        let allowsMultiple = details?["allows_multiple"]?.boolValue ?? false
        let options = event.options.map {
            AskUserChoiceOption(
                value: $0.optionID,
                label: $0.label,
                description: $0.description
            )
        }
        let choice = options.isEmpty ? nil : AskUserChoice(
            allowsMultiple: allowsMultiple,
            options: options,
            minimumSelectionCount: 1,
            maximumSelectionCount: allowsMultiple ? options.count : 1
        )
        let fields = options.isEmpty
            ? [AskUserField(
                key: "answer",
                label: "回复",
                placeholder: "告诉 AI 你的决定或补充信息",
                isRequired: true,
                isMultiline: true
            )]
            : []
        return AskUserPrompt(
            id: event.interactionID,
            sessionID: binding.threadID,
            turnID: binding.turnID,
            kind: kind,
            status: .pending,
            title: title,
            message: event.prompt,
            allowsCancel: allowsCancel,
            fields: fields,
            choice: choice,
            createdAt: localAgentDate(emittedAt),
            updatedAt: localAgentDate(emittedAt)
        )
    }

    private static func localAgentPromptOrder(
        _ lhs: AskUserPrompt,
        _ rhs: AskUserPrompt
    ) -> Bool {
        let left = lhs.createdAt ?? lhs.updatedAt ?? .distantPast
        let right = rhs.createdAt ?? rhs.updatedAt ?? .distantPast
        if left != right { return left < right }
        return lhs.id < rhs.id
    }

    private static func turnStatus(_ status: LocalAgentRunStatus) -> TurnStatus {
        switch status {
        case .queued: .queued
        case .modelReady, .modelRunning, .waitingToolResult, .continuationReady,
             .retryScheduled, .paused, .needsReview: .streaming
        case .succeeded: .completed
        case .failed: .failed
        case .cancelled: .cancelled
        }
    }

    private static func runTitle(_ status: LocalAgentRunStatus) -> String {
        switch status {
        case .queued: "等待本地 Agent"
        case .modelReady: "准备调用模型"
        case .modelRunning: "模型正在生成"
        case .waitingToolResult: "等待工具结果"
        case .continuationReady: "准备继续"
        case .retryScheduled: "等待重试"
        case .paused: "运行已暂停"
        case .needsReview: "需要人工复核"
        case .succeeded: "本地 Agent 已完成"
        case .failed: "本地 Agent 失败"
        case .cancelled: "本地 Agent 已取消"
        }
    }

    private static func runDetail(_ run: LocalAgentRunSnapshot) -> String? {
        var values = ["第 \(run.stepSeq) 步"]
        if run.retryCount > 0 { values.append("已重试 \(run.retryCount) 次") }
        if let reason = run.terminalOutcome?.stringValue(forKey: "reason")?.nonEmpty {
            values.append(reason)
        }
        return values.joined(separator: " · ")
    }

    private static func toolStatus(_ status: LocalAgentToolExecutionStatus) -> TurnStatus {
        switch status {
        case .requested, .awaitingApproval, .approved: .queued
        case .started: .streaming
        case .succeeded: .completed
        case .failed, .rejected, .outcomeUnknown: .failed
        }
    }

    private static func toolDetail(_ tool: LocalAgentToolSnapshot) -> String? {
        var values: [String] = []
        switch tool.status {
        case .requested: values.append("已请求")
        case .awaitingApproval: values.append("等待授权")
        case .approved: values.append("已授权")
        case .started: values.append("执行中")
        case .succeeded: values.append("执行成功")
        case .failed: values.append("执行失败")
        case .rejected: values.append("已拒绝")
        case .outcomeUnknown: values.append("结果未知，需要复核")
        }
        if let reason = tool.approvalReason?.nonEmpty { values.append(reason) }
        if let result = tool.boundedResult,
           let data = try? JSONEncoder().encode(result),
           let rendered = String(data: data, encoding: .utf8)?.nonEmpty {
            values.append(rendered)
        }
        return values.joined(separator: "\n")
    }

    private static func memoryDetail(_ status: LocalAgentMemorySyncStatus) -> String {
        var values = ["待同步 \(status.pendingCount) 条"]
        if status.failedCount > 0 { values.append("失败 \(status.failedCount) 条") }
        if let code = status.lastErrorCode?.nonEmpty { values.append(code) }
        return values.joined(separator: " · ")
    }
}

extension ConversationHistoryStore: LocalAgentUIEventApplying {}
extension ConversationHistoryStore: LocalAgentConversationUpdateStreaming {}
extension ConversationHistoryStore: LocalAgentAskUserStateStoring {}

public enum LocalAgentConversationHistoryError: Error, Equatable, Sendable {
    case runBindingMismatch
    case userMessageBindingMismatch
    case missingSuccessfulOutcome
    case promptUnavailable
}

extension LocalAgentConversationHistoryError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .runBindingMismatch: "本地 Agent 事件与 Run 绑定不一致"
        case .userMessageBindingMismatch: "本地 Agent 用户消息身份不一致"
        case .missingSuccessfulOutcome: "本地 Agent 成功结果缺少最终文本"
        case .promptUnavailable: "这个本地 Agent 提问已经处理或不存在"
        }
    }
}

private extension TurnStatus {
    var isTerminal: Bool {
        self == .completed || self == .failed || self == .cancelled
    }
}

private extension LocalAgentUIEventPayload {
    var runID: String? {
        switch self {
        case let .runSnapshot(run): run.runID
        case let .modelStream(stream): stream.runID
        case let .toolSnapshot(tool): tool.runID
        case let .userInteraction(interaction): interaction.runID
        case let .memorySync(status): status.runID
        case .hostStatus: nil
        }
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}

private extension LocalAgentJSONValue {
    func stringValue(forKey key: String) -> String? {
        guard case let .object(object) = self,
              case let .string(value)? = object[key]
        else { return nil }
        return value
    }

    var boolValue: Bool? {
        guard case let .bool(value) = self else { return nil }
        return value
    }

    var objectValue: [String: LocalAgentJSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    var arrayValue: [LocalAgentJSONValue]? {
        guard case let .array(value) = self else { return nil }
        return value
    }

    var plainString: String? {
        guard case let .string(value) = self else { return nil }
        return value
    }

    var integerValue: UInt64? {
        switch self {
        case let .unsigned(value): value
        case let .signed(value) where value >= 0: UInt64(value)
        default: nil
        }
    }
}

private extension ConversationTurn {
    static func isOrderedBefore(_ lhs: ConversationTurn, _ rhs: ConversationTurn) -> Bool {
        let lhsHasStartedAt = lhs.startedAt != .distantPast
        let rhsHasStartedAt = rhs.startedAt != .distantPast

        // Some older gateways do not return sequence_no. The mapper can only assign a
        // page-local fallback in that case, so sequence values repeat after loading an
        // older page. A real creation time is therefore the stable cross-page order.
        if lhsHasStartedAt, rhsHasStartedAt, lhs.startedAt != rhs.startedAt {
            return lhs.startedAt < rhs.startedAt
        }

        // Preserve server ordering for legacy records with no usable timestamp and use
        // it as the tie-breaker when multiple turns share the same creation time.
        if lhs.sequence != rhs.sequence {
            return lhs.sequence < rhs.sequence
        }

        return lhs.id < rhs.id
    }
}
