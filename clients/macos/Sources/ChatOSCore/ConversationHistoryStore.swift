import Foundation

public actor ConversationHistoryStore {
    private struct SessionState: Sendable {
        var turnsByID: [String: ConversationTurn] = [:]
        var lastAppliedLocalAgentEventSequence: UInt64 = 0
        var localAgentPromptsByID: [String: AskUserPrompt] = [:]
        var localAgentPromptRoutes: [String: LocalAgentAskUserRoute] = [:]
        var localAgentRunControlsByID: [String: LocalAgentRunControlState] = [:]
        var localAgentToolApprovalsByID: [String: LocalAgentToolApprovalRequest] = [:]
        var localAgentTurnIDs: Set<String> = []
        var localAgentSnapshotEventSequenceByRunID: [String: UInt64] = [:]
        var viewportAnchor: ViewportAnchor?
        var unreadNewerCount = 0
    }

    private var sessions: [String: SessionState] = [:]
    private var localUpdateContinuations: [
        String: [UUID: AsyncStream<Void>.Continuation]
    ] = [:]

    public init() {}

    /// Clears every account-derived presentation snapshot while preserving
    /// active UI subscriptions so signed-out views are immediately emptied.
    public func reset() {
        let affectedSessionIDs = Set(sessions.keys).union(localUpdateContinuations.keys)
        sessions.removeAll(keepingCapacity: false)
        for sessionID in affectedSessionIDs {
            localUpdateContinuations[sessionID]?.values.forEach { $0.yield(()) }
        }
    }

    public func upsertOptimisticTurn(
        _ turn: ConversationTurn,
        sessionID: String
    ) throws {
        guard turn.sessionID == sessionID, turn.revision == 0 else {
            throw LocalAgentConversationHistoryError.invalidOptimisticTurn
        }
        var state = sessions[sessionID] ?? SessionState()
        if let existing = state.turnsByID[turn.id] {
            guard existing.revision == 0, existing == turn else {
                throw LocalAgentConversationHistoryError.invalidOptimisticTurn
            }
        } else {
            state.turnsByID[turn.id] = turn
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

    public func localAgentRunControls(sessionID: String) -> [LocalAgentRunControlState] {
        guard let controls = sessions[sessionID]?.localAgentRunControlsByID.values else {
            return []
        }
        return controls
            .filter { !$0.isTerminal }
            .sorted(by: Self.localAgentRunControlOrder)
    }

    public func localAgentPendingToolApprovals(
        sessionID: String
    ) -> [LocalAgentToolApprovalRequest] {
        guard let approvals = sessions[sessionID]?.localAgentToolApprovalsByID.values else {
            return []
        }
        return approvals.sorted { lhs, rhs in
            if lhs.turnID != rhs.turnID { return lhs.turnID < rhs.turnID }
            return lhs.invocationID < rhs.invocationID
        }
    }

    public func requireLocalAgentRunControl(
        runID: String,
        sessionID: String
    ) throws -> LocalAgentRunControlState {
        guard let control = sessions[sessionID]?.localAgentRunControlsByID[runID],
              !control.isTerminal
        else {
            throw LocalAgentConversationHistoryError.runUnavailable
        }
        return control
    }

    public func requireLocalAgentToolApproval(
        invocationID: String,
        sessionID: String
    ) throws -> LocalAgentToolApprovalRequest {
        guard let approval = sessions[sessionID]?.localAgentToolApprovalsByID[invocationID]
        else {
            throw LocalAgentConversationHistoryError.toolApprovalUnavailable
        }
        return approval
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
        if let snapshotSequence = state.localAgentSnapshotEventSequenceByRunID[binding.runID],
           event.eventSeq <= snapshotSequence {
            state.lastAppliedLocalAgentEventSequence = event.eventSeq
            sessions[binding.threadID] = state
            return
        }
        state.localAgentSnapshotEventSequenceByRunID[binding.runID] = nil
        var turn = try localAgentTurn(binding: binding, existing: state.turnsByID[binding.turnID])
        let didChange = try apply(event, to: &turn)
        switch event.event {
        case let .runSnapshot(run):
            state.localAgentRunControlsByID[run.runID] = LocalAgentUIPresentation.runControl(
                run,
                sessionID: binding.threadID,
                turnID: binding.turnID
            )
            if run.status == .failed || run.status == .cancelled || run.status == .succeeded {
                state.localAgentToolApprovalsByID = state.localAgentToolApprovalsByID.filter {
                    $0.value.runID != run.runID
                }
            }
            if run.status == .failed || run.status == .cancelled {
                let resolvedStatus: AskUserPromptStatus = run.status == .cancelled
                    ? .canceled
                    : .failed
                let promptIDs = state.localAgentPromptRoutes.compactMap { id, route in
                    route.runID == run.runID ? id : nil
                }
                for id in promptIDs {
                    state.localAgentPromptsByID[id]?.status = resolvedStatus
                    state.localAgentPromptsByID[id]?.updatedAt = LocalAgentUIPresentation.date(
                        event.emittedAt
                    )
                    state.localAgentPromptRoutes[id] = nil
                }
            }
        case let .toolSnapshot(tool):
            if tool.status == .awaitingApproval {
                state.localAgentToolApprovalsByID[tool.invocationID] =
                    LocalAgentUIPresentation.toolApproval(
                        tool,
                        sessionID: binding.threadID,
                        turnID: binding.turnID
                    )
            } else {
                state.localAgentToolApprovalsByID[tool.invocationID] = nil
            }
        case let .userInteraction(interaction):
            let prompt = LocalAgentUIPresentation.askUserPrompt(
                interaction,
                sessionID: binding.threadID,
                turnID: binding.turnID,
                emittedAt: event.emittedAt
            )
            state.localAgentPromptsByID[prompt.id] = prompt
            state.localAgentPromptRoutes[prompt.id] = LocalAgentAskUserRoute(
                runID: interaction.runID,
                interactionID: interaction.interactionID
            )
        default:
            break
        }
        state.lastAppliedLocalAgentEventSequence = event.eventSeq
        if didChange {
            turn.revision = max(turn.revision, Int64(clamping: event.eventSeq))
            state.turnsByID[turn.id] = turn
            state.localAgentTurnIDs.insert(turn.id)
            if state.viewportAnchor?.isPinnedToBottom == false {
                state.unreadNewerCount += 1
            }
        }
        sessions[binding.threadID] = state
        localUpdateContinuations[binding.threadID]?.values.forEach { $0.yield(()) }
    }

    public func restoreLocalAgentMainChatRuns(
        _ recoveries: [LocalAgentMainChatRunRecovery]
    ) throws {
        var seenRunIDs = Set<String>()
        var seenTurns = Set<String>()
        for recovery in recoveries {
            let binding = recovery.binding
            let run = recovery.detail.run
            guard seenRunIDs.insert(run.runID).inserted,
                  seenTurns.insert("\(binding.threadID):\(binding.turnID)").inserted,
                  binding.runID == run.runID,
                  run.profileKey == "main_chat",
                  run.ownerEntityType == "conversation",
                  run.ownerEntityID == binding.threadID,
                  recovery.detail.tools.allSatisfy({ $0.runID == run.runID })
            else {
                throw LocalAgentConversationHistoryError.runBindingMismatch
            }
        }

        var changedSessions = Set<String>()
        for sessionID in sessions.keys {
            guard var state = sessions[sessionID] else { continue }
            for turnID in state.localAgentTurnIDs {
                state.turnsByID[turnID] = nil
            }
            if !state.localAgentTurnIDs.isEmpty
                || !state.localAgentPromptsByID.isEmpty
                || !state.localAgentRunControlsByID.isEmpty
                || !state.localAgentToolApprovalsByID.isEmpty
            {
                changedSessions.insert(sessionID)
            }
            state.localAgentTurnIDs.removeAll(keepingCapacity: false)
            state.localAgentPromptsByID.removeAll(keepingCapacity: false)
            state.localAgentPromptRoutes.removeAll(keepingCapacity: false)
            state.localAgentRunControlsByID.removeAll(keepingCapacity: false)
            state.localAgentToolApprovalsByID.removeAll(keepingCapacity: false)
            state.localAgentSnapshotEventSequenceByRunID.removeAll(keepingCapacity: false)
            state.lastAppliedLocalAgentEventSequence = 0
            sessions[sessionID] = state
        }

        for recovery in recoveries.sorted(by: Self.localAgentRecoveryOrder) {
            let binding = recovery.binding
            let detail = recovery.detail
            var state = sessions[binding.threadID] ?? SessionState()
            var turn = try localAgentTurn(binding: binding, existing: nil)

            for event in detail.events.sorted(by: Self.localAgentTimelineOrder) {
                applyRecoveredTimelineEvent(event, runID: detail.run.runID, to: &turn)
            }
            for tool in detail.tools {
                _ = try apply(
                    LocalAgentUIEvent(
                        eventSeq: 0,
                        emittedAt: tool.completedAt ?? tool.startedAt ?? detail.run.updatedAt,
                        event: .toolSnapshot(tool)
                    ),
                    to: &turn
                )
                if tool.status == .awaitingApproval {
                    state.localAgentToolApprovalsByID[tool.invocationID] =
                        LocalAgentUIPresentation.toolApproval(
                            tool,
                            sessionID: binding.threadID,
                            turnID: binding.turnID
                        )
                }
            }
            _ = try apply(
                LocalAgentUIEvent(
                    eventSeq: 0,
                    emittedAt: detail.run.updatedAt,
                    event: .runSnapshot(detail.run)
                ),
                to: &turn
            )
            turn.revision = Int64(clamping: detail.run.version)
            state.turnsByID[turn.id] = turn
            state.localAgentTurnIDs.insert(turn.id)
            state.localAgentSnapshotEventSequenceByRunID[detail.run.runID] =
                detail.snapshotEventSequence
            state.localAgentRunControlsByID[detail.run.runID] =
                LocalAgentUIPresentation.runControl(
                    detail.run,
                    sessionID: binding.threadID,
                    turnID: binding.turnID
                )
            if let interaction = LocalAgentUIPresentation.pendingUserInteraction(detail.run) {
                let prompt = LocalAgentUIPresentation.askUserPrompt(
                    interaction,
                    sessionID: binding.threadID,
                    turnID: binding.turnID,
                    emittedAt: detail.run.updatedAt
                )
                state.localAgentPromptsByID[prompt.id] = prompt
                state.localAgentPromptRoutes[prompt.id] = LocalAgentAskUserRoute(
                    runID: detail.run.runID,
                    interactionID: interaction.interactionID
                )
            }
            sessions[binding.threadID] = state
            changedSessions.insert(binding.threadID)
        }

        changedSessions.forEach { sessionID in
            localUpdateContinuations[sessionID]?.values.forEach { $0.yield(()) }
        }
    }

    private func applyRecoveredTimelineEvent(
        _ event: LocalAgentRunTimelineEvent,
        runID: String,
        to turn: inout ConversationTurn
    ) {
        switch event.eventType {
        case "message_assistant_content":
            if let content = event.message {
                appendAssistantText(content, runID: runID, createdAt: event.createdAt, in: &turn)
            }
        case "message_assistant_reasoning":
            if let reasoning = event.message {
                upsertProcessEvent(
                    TurnProcessEvent(
                        id: "local-agent-recovered-\(event.eventID)",
                        title: "思考过程",
                        detail: reasoning,
                        status: .completed
                    ),
                    in: &turn
                )
            }
        default:
            guard !event.eventType.hasPrefix("message_"),
                  !event.eventType.hasPrefix("tool_")
            else { return }
            upsertProcessEvent(
                TurnProcessEvent(
                    id: "local-agent-recovered-\(event.eventID)",
                    title: Self.timelineTitle(event.eventType),
                    detail: event.message,
                    status: Self.timelineStatus(event.eventType)
                ),
                in: &turn
            )
        }
    }

    private static func localAgentRecoveryOrder(
        _ lhs: LocalAgentMainChatRunRecovery,
        _ rhs: LocalAgentMainChatRunRecovery
    ) -> Bool {
        if lhs.binding.threadID != rhs.binding.threadID {
            return lhs.binding.threadID < rhs.binding.threadID
        }
        if lhs.binding.userMessage.sequence != rhs.binding.userMessage.sequence {
            return lhs.binding.userMessage.sequence < rhs.binding.userMessage.sequence
        }
        return lhs.binding.turnID < rhs.binding.turnID
    }

    private static func localAgentTimelineOrder(
        _ lhs: LocalAgentRunTimelineEvent,
        _ rhs: LocalAgentRunTimelineEvent
    ) -> Bool {
        if lhs.createdAt != rhs.createdAt { return lhs.createdAt < rhs.createdAt }
        return lhs.eventID < rhs.eventID
    }

    private static func timelineTitle(_ eventType: String) -> String {
        switch eventType {
        case "run_started": "本地 Agent 已开始"
        case "model_step_requested": "请求模型"
        case "model_step_completed": "模型步骤完成"
        case "tool_batch_requested": "请求工具批次"
        case "tool_batch_completed": "工具批次完成"
        case "continuation_requested": "继续执行"
        case "retry_due": "准备重试"
        case "pause_requested": "暂停运行"
        case "resume_requested": "恢复运行"
        case "cancel_requested": "取消运行"
        case "memory_sync_due": "同步记忆"
        case "run_terminal": "运行结束"
        default: eventType.replacingOccurrences(of: "_", with: " ")
        }
    }

    private static func timelineStatus(_ eventType: String) -> TurnStatus {
        if eventType.contains("failed")
            || eventType.contains("rejected")
            || eventType.contains("outcome_unknown")
        {
            return .failed
        }
        return .completed
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
        let createdAt = LocalAgentUIPresentation.date(message.createdAt) ?? .distantPast
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
            if let startedAt = LocalAgentUIPresentation.date(run.createdAt),
               turn.startedAt == .distantPast {
                turn.startedAt = startedAt
            }
            if run.status == .succeeded {
                guard let text = run.terminalOutcome?.stringValue(forKey: "text")?.nonEmpty else {
                    throw LocalAgentConversationHistoryError.missingSuccessfulOutcome
                }
                setAssistantText(text, runID: run.runID, createdAt: event.emittedAt, in: &turn)
            }
            if status == .completed || status == .failed || status == .cancelled {
                turn.completedAt = LocalAgentUIPresentation.date(run.updatedAt)
                    ?? LocalAgentUIPresentation.date(event.emittedAt)
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
                    id: "local-agent-memory-\(status.runID)",
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
                createdAt: LocalAgentUIPresentation.date(createdAt) ?? Date()
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
                createdAt: LocalAgentUIPresentation.date(createdAt) ?? Date()
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

    private static func localAgentPromptOrder(
        _ lhs: AskUserPrompt,
        _ rhs: AskUserPrompt
    ) -> Bool {
        let left = lhs.createdAt ?? lhs.updatedAt ?? .distantPast
        let right = rhs.createdAt ?? rhs.updatedAt ?? .distantPast
        if left != right { return left < right }
        return lhs.id < rhs.id
    }

    private static func localAgentRunControlOrder(
        _ lhs: LocalAgentRunControlState,
        _ rhs: LocalAgentRunControlState
    ) -> Bool {
        let left = lhs.updatedAt ?? .distantPast
        let right = rhs.updatedAt ?? .distantPast
        if left != right { return left < right }
        return lhs.runID < rhs.runID
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
extension ConversationHistoryStore: LocalAgentMainChatStateRestoring {}
extension ConversationHistoryStore: LocalAgentConversationUpdateStreaming {}
extension ConversationHistoryStore: LocalAgentAskUserStateStoring {}
extension ConversationHistoryStore: LocalAgentRunControlStateStoring {}

public enum LocalAgentConversationHistoryError: Error, Equatable, Sendable {
    case invalidOptimisticTurn
    case runBindingMismatch
    case userMessageBindingMismatch
    case missingSuccessfulOutcome
    case promptUnavailable
    case runUnavailable
    case toolApprovalUnavailable
}

extension LocalAgentConversationHistoryError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .invalidOptimisticTurn: "只能暂存当前会话中尚未持久化的新消息"
        case .runBindingMismatch: "本地 Agent 事件与 Run 绑定不一致"
        case .userMessageBindingMismatch: "本地 Agent 用户消息身份不一致"
        case .missingSuccessfulOutcome: "本地 Agent 成功结果缺少最终文本"
        case .promptUnavailable: "这个本地 Agent 提问已经处理或不存在"
        case .runUnavailable: "这个本地 Agent Run 已结束或不存在"
        case .toolApprovalUnavailable: "这个工具授权请求已经处理或不存在"
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

private extension ConversationTurn {
    static func isOrderedBefore(_ lhs: ConversationTurn, _ rhs: ConversationTurn) -> Bool {
        if lhs.sequence != rhs.sequence {
            return lhs.sequence < rhs.sequence
        }
        if lhs.startedAt != rhs.startedAt { return lhs.startedAt < rhs.startedAt }
        return lhs.id < rhs.id
    }
}
