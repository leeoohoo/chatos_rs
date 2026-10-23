import Foundation

/// Execution boundary used by long-lived Agent tasks. The in-process runtime and the dedicated
/// worker client implement the same contract, so application wiring can require a process boundary
/// without moving SQLite ownership or tool authority into the worker.
public protocol AgentRuntimeExecuting: Sendable {
    func executeRun(
        checkpoint: AgentRunCheckpoint,
        scope: String,
        policy: AgentRunPolicy,
        model: any AgentModelClient,
        tools: [AgentToolDefinition],
        execute: @escaping AgentRuntime.Executor,
        completionCheck: @escaping AgentRuntime.CompletionCheck,
        contextProvider: (any AgentContextProviding)?,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> AgentRunCheckpoint
}

extension AgentRuntime: AgentRuntimeExecuting {
    public func executeRun(
        checkpoint: AgentRunCheckpoint,
        scope: String,
        policy: AgentRunPolicy,
        model: any AgentModelClient,
        tools: [AgentToolDefinition],
        execute: @escaping AgentRuntime.Executor,
        completionCheck: @escaping AgentRuntime.CompletionCheck,
        contextProvider: (any AgentContextProviding)?,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> AgentRunCheckpoint {
        try await run(
            checkpoint: checkpoint,
            scope: scope,
            policy: policy,
            model: model,
            tools: tools,
            execute: execute,
            completionCheck: completionCheck,
            contextProvider: contextProvider,
            record: record
        )
    }
}

/// Preserves retry semantics when a model callback crosses an IPC boundary.
public struct AgentRemoteCallbackError: LocalizedError, Sendable {
    public let detail: String
    public let transient: Bool

    public init(detail: String, transient: Bool) {
        self.detail = detail
        self.transient = transient
    }

    public var errorDescription: String? { detail }
}
