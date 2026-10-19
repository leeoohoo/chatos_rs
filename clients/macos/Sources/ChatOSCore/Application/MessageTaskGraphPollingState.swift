import Foundation

/// Application state for Task Graph recovery and polling decisions.
///
/// The UI owns the cancellable polling Task; this type owns the product rules that decide
/// whether another poll is needed and how aggressively it should run.
public struct MessageTaskGraphPollingState: Sendable, Equatable {
    public static let emptyGraphRetryLimit = 3

    public private(set) var emptyGraphRetryAttemptsRemaining: Int

    public init(
        emptyGraphRetryAttemptsRemaining: Int = Self.emptyGraphRetryLimit
    ) {
        self.emptyGraphRetryAttemptsRemaining = max(0, emptyGraphRetryAttemptsRemaining)
    }

    public func shouldRetryEmptyGraph(
        expectsTaskGraph: Bool,
        graph: MessageTaskGraphSnapshot?
    ) -> Bool {
        expectsTaskGraph
            && graph?.nodes.isEmpty == true
            && emptyGraphRetryAttemptsRemaining > 0
    }

    /// Resets the bounded retry budget after an explicit user refresh.
    /// Returns `nil` when the current graph must not enter recovery mode.
    public mutating func resetRetryBudgetIfNeeded(
        expectsTaskGraph: Bool,
        graph: MessageTaskGraphSnapshot?
    ) -> Bool? {
        guard graph?.nodes.isEmpty != false, expectsTaskGraph else { return nil }
        emptyGraphRetryAttemptsRemaining = Self.emptyGraphRetryLimit
        return graph != nil
    }

    /// Records one completed empty-graph retry and returns the new awaiting state.
    public mutating func recordEmptyGraphRetry(
        expectsTaskGraph: Bool,
        graph: MessageTaskGraphSnapshot?
    ) -> Bool? {
        guard graph?.nodes.isEmpty == true else { return nil }
        emptyGraphRetryAttemptsRemaining = max(
            0,
            emptyGraphRetryAttemptsRemaining - 1
        )
        return shouldRetryEmptyGraph(
            expectsTaskGraph: expectsTaskGraph,
            graph: graph
        )
    }

    /// Applies a graph observation and returns whether the UI is awaiting initial Graph data.
    public mutating func observe(
        graph: MessageTaskGraphSnapshot,
        expectsTaskGraph: Bool
    ) -> Bool {
        if !graph.nodes.isEmpty {
            emptyGraphRetryAttemptsRemaining = 0
            return false
        }
        return shouldRetryEmptyGraph(
            expectsTaskGraph: expectsTaskGraph,
            graph: graph
        )
    }

    public func shouldPoll(
        graph: MessageTaskGraphSnapshot?,
        expectsTaskGraph: Bool
    ) -> Bool {
        graph?.nodes.contains(where: { $0.task.isActive }) == true
            || shouldRetryEmptyGraph(
                expectsTaskGraph: expectsTaskGraph,
                graph: graph
            )
    }

    public static func interval(
        isEmptyGraphRetry: Bool,
        hasActiveRealtimeStream: Bool
    ) -> Duration {
        if isEmptyGraphRetry { return .milliseconds(600) }
        return hasActiveRealtimeStream ? .seconds(60) : .seconds(2)
    }
}
