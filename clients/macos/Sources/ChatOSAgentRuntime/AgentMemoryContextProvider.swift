import Foundation

public struct AgentPreparedContext: Codable, Equatable, Sendable {
    public let checkpoint: AgentRunCheckpoint
    public let messages: [AgentMessage]

    public init(checkpoint: AgentRunCheckpoint, messages: [AgentMessage]) {
        self.checkpoint = checkpoint
        self.messages = messages
    }
}

public struct AgentContextPreparationFailure: Error, Sendable {
    public let checkpoint: AgentRunCheckpoint
    public let reason: String
    public let cancelled: Bool

    public init(checkpoint: AgentRunCheckpoint, reason: String, cancelled: Bool) {
        self.checkpoint = checkpoint
        self.reason = reason
        self.cancelled = cancelled
    }
}

public protocol AgentContextProviding: Sendable {
    var scope: AgentMemoryScope { get }
    func prepare(
        checkpoint initial: AgentRunCheckpoint,
        tools: [AgentToolDefinition],
        modelCapability: AgentModelCapabilitySnapshot,
        deadline: Date,
        synchronizeOnly: Bool,
        shouldPause: @escaping @Sendable () async -> Bool,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> AgentPreparedContext
    func finalizeMemory() async throws
}

public extension AgentContextProviding {
    func finalizeMemory() async throws {}
}

/// No credentials or network configuration are stored in checkpoints.
public struct AgentMemoryContextProvider: AgentContextProviding, Sendable {
    private static let maximumRequiredCompactionRounds = 3
    public let scope: AgentMemoryScope
    private let service: any AgentMemoryServicing

    public init(scope: AgentMemoryScope, service: any AgentMemoryServicing) {
        self.scope = scope; self.service = service
    }

    /// Binds the application-selected Memory Engine scope to a durable checkpoint.
    public func bind(_ checkpoint: AgentRunCheckpoint) throws -> AgentRunCheckpoint {
        guard checkpoint.id == scope.runID, checkpoint.scope == scope.runtimeScope else { throw AgentRuntimeError.scopeMismatch }
        var checkpoint = checkpoint
        if let memory = checkpoint.memory {
            guard memory.scope == scope else { throw AgentRuntimeError.scopeMismatch }
        } else {
            guard checkpoint.modelCalls == 0, checkpoint.pendingCalls.isEmpty, !checkpoint.messages.isEmpty,
                  checkpoint.messages.allSatisfy({ [.system, .user].contains($0.role) && $0.toolCalls.isEmpty && $0.toolCallID == nil }) else {
                throw AgentContextError.invalidHistory
            }
            checkpoint.memory = .init(scope: scope, pinnedMessageCount: checkpoint.messages.count)
        }
        return checkpoint
    }

    public func finalizeMemory() async throws {
        guard scope.projectAggregationSubjectID != nil else { return }
        try await service.flushSummary()
    }

    public func prepare(checkpoint initial: AgentRunCheckpoint, tools: [AgentToolDefinition],
                        modelCapability: AgentModelCapabilitySnapshot,
                        deadline: Date, synchronizeOnly: Bool = false,
                        shouldPause: @escaping @Sendable () async -> Bool = { false },
                        record: @escaping AgentRuntime.Recorder) async throws -> AgentPreparedContext {
        var state = initial
        do {
            guard let bound = state.memory, bound.scope == scope, state.scope == scope.runtimeScope,
                  state.id == scope.runID, bound.syncedMessageCount >= 0,
                  bound.syncedMessageCount <= state.messages.count else { throw AgentContextError.invalidHistory }
            // A write-only flush is valid while an assistant tool call is pending:
            // the call itself must reach Memory Engine before tool execution. Only
            // context composition requires a quiescent call/result boundary.
            if !synchronizeOnly,
               (!state.pendingCalls.isEmpty || state.inFlightCallID != nil) {
                throw AgentContextError.invalidHistory
            }
            if bound.syncedMessageCount > 0 {
                guard try AgentContextBudget.digest(state.messages.prefix(bound.syncedMessageCount)) == bound.syncedDigest else {
                    throw AgentContextError.invalidHistory
                }
            }
            try await check(deadline: deadline, shouldPause: shouldPause)
            if !bound.threadCreated {
                try await emit(state, "memory_connecting", "正在连接当前运行的 Memory Engine 历史", record: record)
                try await service.ensureThread()
                state.memory!.threadCreated = true
                try await emit(state, "memory_connected", "运行历史已连接", record: record)
            }
            while state.memory!.syncedMessageCount < state.messages.count {
                try await check(deadline: deadline, shouldPause: shouldPause)
                let start = state.memory!.syncedMessageCount
                let reconciling = state.memory!.syncInFlightEnd != nil
                let end = state.memory!.syncInFlightEnd ?? min(start + 32, state.messages.count)
                guard end > start, end <= state.messages.count, end - start <= 32 else { throw AgentContextError.invalidHistory }
                let entries = try (start..<end).map { index in
                    AgentMemoryEntry(
                        id: scope.recordID(at: index),
                        index: index,
                        message: state.messages[index],
                        createdAt: bound.recordEpoch.addingTimeInterval(Double(index) / 1_000),
                        conversationTurnID: conversationTurnID(state.messages, index: index),
                        toolExchange: try toolExchange(state.messages, index: index)
                    )
                }
                state.memory!.syncInFlightEnd = end
                try await emit(state, "memory_syncing", "正在同步运行记录 \(start + 1)–\(end)", record: record)
                try await service.sync(entries, reconciling: reconciling)
                state.memory!.syncedMessageCount = end
                state.memory!.syncInFlightEnd = nil
                state.memory!.syncedDigest = try AgentContextBudget.digest(state.messages.prefix(end))
                try await emit(state, "memory_synced", "已同步 \(end) 条运行记录", record: record)
            }
            if synchronizeOnly { return .init(checkpoint: state, messages: []) }
            try await check(deadline: deadline, shouldPause: shouldPause)

            // The engine owns durable records and summaries. This same compose path is used at
            // turn boundaries and when an active tool loop must shrink its recalled history.
            let currentUserIndex = try AgentContextAssembler.currentTurnStart(in: state.messages)
            let fixedMessages = try AgentContextAssembler.assembleFixed(
                checkpoint: state, memory: state.memory!
            )
            let fixedTokens = try AgentContextBudget.estimate(messages: fixedMessages, tools: tools)
            let available = modelCapability.contextWindowTokens - modelCapability.outputReserveTokens - fixedTokens
            // A live turn may itself consume the whole soft recall budget. Memory Engine rejects
            // zero-token budgets, and asking it to summarize into one token is both invalid in
            // practice and unnecessary. Return the exact fixed projection and let AgentRuntime
            // decide whether Responses server-side compaction may carry it up to the hard window.
            guard available > 0 else {
                try await emit(
                    state,
                    "memory_recall_skipped",
                    "当前活动轮次已占满 Memory 召回预算，未附加历史召回",
                    record: record
                )
                try await check(deadline: deadline, shouldPause: shouldPause)
                return .init(checkpoint: state, messages: fixedMessages)
            }
            let composeRequest = try AgentMemoryComposeRequest(
                modelCapability: modelCapability,
                queryText: state.messages[currentUserIndex].content,
                currentTurnID: conversationTurnID(state.messages, index: currentUserIndex),
                availableMemoryTokens: available
            )
            var context = try await service.compose(composeRequest)
            for round in 1...Self.maximumRequiredCompactionRounds where context.needsCompaction {
                try await emit(
                    state,
                    "memory_compacting",
                    "Memory Engine 正在压缩超出预算的历史上下文（\(round)/\(Self.maximumRequiredCompactionRounds)）",
                    record: record
                )
                let beforeTokens = context.budgetUsedTokens
                let beforePending = context.pendingRecordCount ?? context.recentRecords.count
                do {
                    try await service.compactSummary()
                } catch let failure as AgentMemoryCompactionFailure {
                    try await emit(
                        state,
                        "memory_compaction_deferred",
                        "Memory Engine 历史压缩暂时失败；本轮仅使用固定指令和完整当前活动轮继续，历史记录仍保留并等待后续重试。\(failure.reason.prefix(400))",
                        record: record
                    )
                    try await check(deadline: deadline, shouldPause: shouldPause)
                    return .init(checkpoint: state, messages: fixedMessages)
                }
                try await check(deadline: deadline, shouldPause: shouldPause)
                let recomposed = try await service.compose(composeRequest)
                let afterPending = recomposed.pendingRecordCount ?? recomposed.recentRecords.count
                if recomposed.needsCompaction,
                   afterPending >= beforePending,
                   let beforeTokens,
                   let afterTokens = recomposed.budgetUsedTokens,
                   afterTokens >= beforeTokens {
                    throw AgentContextError.budgetExceeded
                }
                context = recomposed
            }
            guard !context.needsCompaction else { throw AgentContextError.budgetExceeded }
            let messages = try AgentContextAssembler.assemble(
                checkpoint: state, memory: state.memory!, context: context
            )
            try await check(deadline: deadline, shouldPause: shouldPause)
            return .init(checkpoint: state, messages: messages)
        } catch {
            // Preserve the last acknowledged record-sync cursor on every exit. Summary progress
            // is recovered from Memory Engine itself on the next prepare call.
            throw AgentContextPreparationFailure(checkpoint: state, reason: error.localizedDescription,
                                                 cancelled: error is CancellationError)
        }
    }

    private func conversationTurnID(_ messages: [AgentMessage], index: Int) -> String {
        let start = messages[...index].lastIndex(where: { $0.role == .user }) ?? index
        return "agent:\(scope.runID.uuidString):turn:\(start)"
    }

    private func toolExchange(_ messages: [AgentMessage], index: Int) throws -> AgentMemoryToolExchange? {
        let message = messages[index]
        guard message.role == .tool else {
            guard message.toolCallID == nil else { throw AgentContextError.invalidHistory }
            return nil
        }
        guard let callID = message.toolCallID,
              let assistantIndex = messages[..<index].lastIndex(where: { candidate in
                  candidate.role == .assistant && candidate.toolCalls.contains(where: { $0.id == callID })
              }),
              let ordinal = messages[assistantIndex].toolCalls.firstIndex(where: { $0.id == callID }) else {
            throw AgentContextError.invalidHistory
        }
        let calls = messages[assistantIndex].toolCalls
        return try .init(
            assistantRecordID: scope.recordID(at: assistantIndex),
            exchangeID: "agent:\(scope.runID.uuidString):tool:\(assistantIndex)",
            callID: callID,
            resultOrdinal: ordinal,
            resultCount: calls.count
        )
    }

    private func check(deadline: Date, shouldPause: @escaping @Sendable () async -> Bool) async throws {
        try Task.checkCancellation()
        if await shouldPause() { throw CancellationError() }
        guard Date() < deadline else { throw AgentRuntimeError.timeout }
    }

    private func emit(_ checkpoint: AgentRunCheckpoint, _ kind: String, _ detail: String,
                      record: @escaping AgentRuntime.Recorder) async throws {
        try await record(checkpoint, .init(kind: kind, detail: detail, modelCalls: checkpoint.modelCalls))
    }
}
