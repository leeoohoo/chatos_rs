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
                        policy: AgentContextPolicy,
                        deadline: Date, synchronizeOnly: Bool = false,
                        shouldPause: @escaping @Sendable () async -> Bool = { false },
                        record: @escaping AgentRuntime.Recorder) async throws -> AgentPreparedContext {
        var state = initial
        do {
            try policy.validate()
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

            // The engine owns durable records and background summaries. It is
            // composed at this turn boundary, but it is never used as an
            // in-loop overflow recovery mechanism.
            let messages = try AgentContextAssembler.assemble(
                checkpoint: state, memory: state.memory!, context: await service.compose()
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
