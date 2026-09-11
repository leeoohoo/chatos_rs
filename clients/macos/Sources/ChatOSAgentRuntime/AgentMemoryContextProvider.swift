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

            // This is the same boundary used by AiRuntime: observe a server-owned in-flight job
            // before composing input. The checkpoint never owns a second summary state machine.
            do {
                if try await waitForInflightSummary(checkpoint: state, policy: policy, deadline: deadline,
                                                    shouldPause: shouldPause, record: record) {
                    state.memory!.compactions += 1
                }
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                // AiRuntime warns and continues here. Compose remains authoritative and will still
                // fail closed if Memory Engine itself is unavailable.
                try await emit(state, "context_summary_check_failed",
                               "无法确认在途摘要状态，本轮继续从 Memory Engine compose：\(error.localizedDescription)", record: record)
            }

            var messages = try AgentContextAssembler.assemble(checkpoint: state, memory: state.memory!,
                                                                context: await service.compose())
            var count = try AgentContextBudget.estimate(messages: messages, tools: tools)
            var force = forceCompaction
            for _ in 0..<policy.maximumCompactionPasses where count > policy.compactionThresholdTokens || force {
                try await check(deadline: deadline, shouldPause: shouldPause)
                let before = count
                let changed: Bool
                do {
                    changed = try await compactActiveContext(
                        checkpoint: state, reason: force ? "context_overflow" : "active_context_budget",
                        policy: policy, deadline: deadline, shouldPause: shouldPause, record: record
                    )
                } catch is CancellationError {
                    throw CancellationError()
                } catch {
                    if force || count > policy.hardInputLimit { throw error }
                    try await emit(state, "context_summary_skipped",
                                   "主动摘要失败，但输入仍在硬限制内，本轮继续：\(error.localizedDescription)", record: record)
                    break
                }
                guard changed else {
                    if force || count > policy.hardInputLimit { throw AgentContextError.noImprovement }
                    try await emit(state, "context_summary_skipped",
                                   "Memory Engine 未生成摘要或压缩记录；输入仍在硬限制内，本轮不重复提交", record: record)
                    break
                }
                state.memory!.compactions += 1
                try await emit(state, "context_summary_completed", "摘要任务完成，正在从 Memory Engine 重新 compose 上下文", record: record)
                try await check(deadline: deadline, shouldPause: shouldPause)
                messages = try AgentContextAssembler.assemble(checkpoint: state, memory: state.memory!,
                                                               context: await service.compose())
                count = try AgentContextBudget.estimate(messages: messages, tools: tools)
                force = false
                try await emit(state, "context_compacted", "模型输入估算由 \(before) 降至 \(count) tokens", record: record)
            }
            guard count <= policy.hardInputLimit else { throw AgentContextError.budgetExceeded }
            try await check(deadline: deadline, shouldPause: shouldPause)
            return .init(checkpoint: state, messages: messages)
        } catch {
            // Preserve the last acknowledged record-sync cursor on every exit. Summary progress
            // is recovered from Memory Engine itself on the next prepare call.
            throw AgentContextPreparationFailure(checkpoint: state, reason: error.localizedDescription,
                                                 cancelled: error is CancellationError)
        }
    }

    private func check(deadline: Date, shouldPause: @escaping @Sendable () async -> Bool) async throws {
        try Task.checkCancellation()
        if await shouldPause() { throw CancellationError() }
        guard Date() < deadline else { throw AgentRuntimeError.timeout }
    }

    private func waitForInflightSummary(
        checkpoint: AgentRunCheckpoint, policy: AgentContextPolicy, deadline: Date,
        shouldPause: @escaping @Sendable () async -> Bool,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> Bool {
        let initial = try await service.summaryStatus(jobID: nil)
        guard initial.running else { return false }
        try await emit(checkpoint, "context_summary_waiting",
                       "检测到当前线程正在压缩上下文，暂停新的模型请求", record: record)
        let status = try await waitForSummary(initial, policy: policy, deadline: deadline,
                                              shouldPause: shouldPause)
        if status.failed {
            try await emit(checkpoint, "context_summary_skipped",
                           "在途摘要任务失败，本轮将使用 Memory Engine 当前可 compose 的上下文", record: record)
            return false
        }
        return status.changedContext
    }

    private func compactActiveContext(
        checkpoint: AgentRunCheckpoint, reason: String, policy: AgentContextPolicy, deadline: Date,
        shouldPause: @escaping @Sendable () async -> Bool,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> Bool {
        try await emit(checkpoint, "context_compacting",
                       reason == "context_overflow"
                        ? "模型报告上下文溢出，正在请求 Memory Engine 压缩"
                        : "上下文达到主动压缩阈值，正在请求 Memory Engine 压缩",
                       record: record)
        let initial = try await service.startSummary(reason: reason)
        let status = try await waitForSummary(initial, policy: policy, deadline: deadline,
                                              shouldPause: shouldPause)
        if status.failed { throw AgentContextError.summaryFailed(status.errorMessage) }
        return status.changedContext
    }

    private func waitForSummary(
        _ initial: AgentSummaryStatus, policy: AgentContextPolicy, deadline: Date,
        shouldPause: @escaping @Sendable () async -> Bool
    ) async throws -> AgentSummaryStatus {
        if initial.completed || initial.failed || !initial.running { return initial }
        var status = initial
        let summaryDeadline = min(deadline, Date().addingTimeInterval(Double(policy.summaryTimeoutSeconds)))
        while status.running {
            try await check(deadline: deadline, shouldPause: shouldPause)
            guard Date() < summaryDeadline else { throw AgentContextError.summaryTimedOut }
            let delay = min(Double(policy.summaryPollSeconds), max(0, summaryDeadline.timeIntervalSinceNow))
            guard delay > 0 else { throw AgentContextError.summaryTimedOut }
            let nanoseconds = UInt64(min(Double(UInt64.max), (delay * 1_000_000_000).rounded(.up)))
            // Keep suspension in the current task frame. This also avoids the Swift 6.3
            // cancellation/deallocation crash seen in the previous escaping sleep closure.
            do { try await Task<Never, Never>.sleep(nanoseconds: nanoseconds) }
            catch { throw CancellationError() }
            try await check(deadline: deadline, shouldPause: shouldPause)
            guard Date() < summaryDeadline else { throw AgentContextError.summaryTimedOut }
            status = try await service.summaryStatus(jobID: initial.jobID)
        }
        return status
    }

    private func emit(_ checkpoint: AgentRunCheckpoint, _ kind: String, _ detail: String,
                      record: @escaping AgentRuntime.Recorder) async throws {
        try await record(checkpoint, .init(kind: kind, detail: detail, modelCalls: checkpoint.modelCalls))
    }
}
