import Foundation

public struct LocalConnectorApprovalMonitoringSession: Sendable, Equatable {
    public let generation: UInt64
    public let hasStreamingService: Bool

    public var shouldRefreshImmediately: Bool {
        !hasStreamingService
    }

    public var consistencyCheckInterval: Duration {
        LocalConnectorApprovalMonitoringPolicy.consistencyCheckInterval(
            hasStreamingService: hasStreamingService
        )
    }

    public init(
        generation: UInt64,
        hasStreamingService: Bool
    ) {
        self.generation = generation
        self.hasStreamingService = hasStreamingService
    }
}

/// Pure lifecycle state for approval snapshot, event, and consistency tasks.
/// The client owns the concrete Tasks; this state supplies a session token so
/// updates from a stopped monitor can never write into a later account session.
public struct LocalConnectorApprovalMonitoringState: Sendable, Equatable {
    public private(set) var activeSession: LocalConnectorApprovalMonitoringSession?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func start(
        hasStreamingService: Bool
    ) -> LocalConnectorApprovalMonitoringSession {
        if let activeSession {
            return activeSession
        }
        lastGeneration &+= 1
        let session = LocalConnectorApprovalMonitoringSession(
            generation: lastGeneration,
            hasStreamingService: hasStreamingService
        )
        activeSession = session
        return session
    }

    public mutating func stop() {
        activeSession = nil
    }

    public func accepts(
        _ session: LocalConnectorApprovalMonitoringSession
    ) -> Bool {
        activeSession == session
    }
}
