import Foundation

/// Application use case for loading the Task Graph and deciding when realtime events
/// require a durable graph refresh.
public struct MessageTaskGraphUseCase: Sendable {
    private let graphService: any MessageTaskGraphServicing

    public init(graphService: any MessageTaskGraphServicing) {
        self.graphService = graphService
    }

    public func loadTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTask {
        try await graphService.fetchTask(
            messageID: messageID,
            taskID: taskID,
            lookup: lookup
        )
    }

    /// Loads the latest graph without allowing a transient empty gateway response to erase
    /// a graph that has already been observed for this message.
    public func loadGraph(
        messageID: String,
        lookup: MessageTaskLookup?,
        preservingStable current: MessageTaskGraphSnapshot?
    ) async throws -> MessageTaskGraphSnapshot {
        let fetched = try await graphService.fetchGraph(
            messageID: messageID,
            lookup: lookup
        )
        guard fetched.nodes.isEmpty,
              let current,
              !current.nodes.isEmpty else {
            return fetched
        }
        return current
    }

    /// Reconcile is session-wide. Terminal and persistence signals refresh only the turn
    /// whose Task Graph they belong to.
    public func shouldRefreshWorkspace(
        for signal: ConversationRealtimeSignal,
        turnID: String
    ) -> Bool {
        if signal.kind == .reconcile { return true }
        guard signal.turnID == turnID else { return false }
        return switch signal.kind {
        case .completed, .failed, .cancelled, .persisted:
            true
        case .reconcile, .started, .updated, .unknown:
            false
        }
    }

    @discardableResult
    public func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun {
        try await graphService.retryRun(
            messageID: messageID,
            runID: runID,
            lookup: lookup,
            instruction: instruction
        )
    }
}
