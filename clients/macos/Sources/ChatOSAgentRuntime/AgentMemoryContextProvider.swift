import Foundation

public struct AgentPreparedContext: Sendable {
    public let checkpoint: AgentRunCheckpoint
    public let messages: [AgentMessage]
}

public struct AgentContextPreparationFailure: Error, Sendable {
    public let checkpoint: AgentRunCheckpoint
    public let reason: String
    public let cancelled: Bool
}

/// No credentials or network configuration are stored in checkpoints.
public struct AgentMemoryContextProvider: Sendable {
    public let scope: AgentMemoryScope
    private let service: any AgentMemoryServicing
    private let sleep: @Sendable (UInt64) async -> Void

    public init(scope: AgentMemoryScope, service: any AgentMemoryServicing,
                sleep: @escaping @Sendable (UInt64) async -> Void = {
                    // Consume cancellation inside the suspended closure. Propagating that
                    // error across an escaping async closure trips Swift 6.3 task teardown.
                    try? await Task<Never, Never>.sleep(nanoseconds: $0)
                }) {
        self.scope = scope; self.service = service; self.sleep = sleep
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

    public func prepare(checkpoint initial: AgentRunCheckpoint, tools: [AgentToolDefinition],
                        policy: AgentContextPolicy, forceCompaction: Bool = false,
                        deadline: Date, synchronizeOnly: Bool = false,
                        shouldPause: @escaping @Sendable () async -> Bool = { false },
                        record: @escaping AgentRuntime.Recorder) async throws -> AgentPreparedContext {
        var state = initial
        do {
            try policy.validate()
            guard let bound = state.memory, bound.scope == scope, state.scope == scope.runtimeScope,
                  state.id == scope.runID, bound.syncedMessageCount >= 0,
                  bound.syncedMessageCount <= state.messages.count,
                  state.pendingCalls.isEmpty, state.inFlightCallID == nil else { throw AgentContextError.invalidHistory }
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
                let entries = (start..<end).map { index in
                    AgentMemoryEntry(id: scope.recordID(at: index), index: index, message: state.messages[index],
                                     createdAt: bound.recordEpoch.addingTimeInterval(Double(index) / 1_000))
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
            var messages = try AgentContextAssembler.assemble(checkpoint: state, memory: state.memory!, context: await service.compose())
            var count = try AgentContextBudget.estimate(messages: messages, tools: tools)
            var force = forceCompaction
            var passes = 0
            while count > policy.compactionThresholdTokens || force || state.memory!.summaryRequested {
                try await check(deadline: deadline, shouldPause: shouldPause)
                guard passes < policy.maximumCompactionPasses else { throw AgentContextError.budgetExceeded }
                passes += 1
                let before = state.memory!.summaryInputEstimate ?? count
                var status: AgentSummaryStatus
                if state.memory!.summaryRequested {
                    status = try await service.summaryStatus(jobID: state.memory!.summaryJobID)
                    if !status.running && !status.completed && !status.failed {
                        status = try await service.startSummary(reason: "active_context_budget")
                    }
                } else {
                    state.memory!.summaryRequested = true
                    state.memory!.summaryInputEstimate = count
                    try await emit(state, "context_compacting", "上下文接近预算，正在请求 Memory Engine 压缩", record: record)
                    status = try await service.startSummary(reason: force ? "context_overflow" : "active_context_budget")
                }
                state.memory!.summaryJobID = status.jobID
                try await emit(state, "context_summary_waiting", "正在等待 Memory Engine 摘要任务", record: record)
                let summaryDeadline = min(deadline, Date().addingTimeInterval(Double(policy.summaryTimeoutSeconds)))
                while status.running {
                    try await check(deadline: deadline, shouldPause: shouldPause)
                    guard Date() < summaryDeadline else { throw AgentContextError.summaryTimedOut }
                    let remainingSeconds = max(0, summaryDeadline.timeIntervalSinceNow)
                    let delaySeconds = min(Double(policy.summaryPollSeconds), remainingSeconds)
                    guard delaySeconds > 0 else { throw AgentContextError.summaryTimedOut }
                    let delayNanoseconds = UInt64(min(Double(UInt64.max), (delaySeconds * 1_000_000_000).rounded(.up)))
                    await sleep(delayNanoseconds)
                    try await check(deadline: deadline, shouldPause: shouldPause)
                    guard Date() < summaryDeadline else { throw AgentContextError.summaryTimedOut }
                    status = try await service.summaryStatus(jobID: state.memory!.summaryJobID)
                }
                guard !status.failed, status.completed else {
                    throw AgentContextError.summaryFailed(status.errorMessage)
                }
                state.memory!.summaryRequested = false
                state.memory!.summaryJobID = nil
                state.memory!.summaryInputEstimate = nil
                state.memory!.compactions += 1
                try await emit(state, "context_summary_completed", "摘要任务结束，正在重新检查输入预算", record: record)
                try await check(deadline: deadline, shouldPause: shouldPause)
                messages = try AgentContextAssembler.assemble(checkpoint: state, memory: state.memory!, context: await service.compose())
                count = try AgentContextBudget.estimate(messages: messages, tools: tools)
                // A completed/no-op job is not proof of useful compaction.
                guard count < before else { throw AgentContextError.noImprovement }
                force = false
                try await emit(state, "context_compacted", "上下文安全估计由 \(before) 降至 \(count)，不是精确 token 数", record: record)
            }
            guard count <= policy.hardInputLimit else { throw AgentContextError.budgetExceeded }
            try await check(deadline: deadline, shouldPause: shouldPause)
            return .init(checkpoint: state, messages: messages)
        } catch {
            // Preserve the last acknowledged sync cursor and in-flight summary ID on every exit.
            throw AgentContextPreparationFailure(checkpoint: state, reason: error.localizedDescription,
                                                 cancelled: error is CancellationError)
        }
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
