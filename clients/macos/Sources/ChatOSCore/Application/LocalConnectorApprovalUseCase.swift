import Foundation

public struct LocalConnectorApprovalSnapshot: Sendable, Equatable {
    public var settings: LocalConnectorApprovalSettings
    public var pendingApprovals: [LocalConnectorPendingApproval]

    public init(
        settings: LocalConnectorApprovalSettings,
        pendingApprovals: [LocalConnectorPendingApproval]
    ) {
        self.settings = settings
        self.pendingApprovals = pendingApprovals
    }
}

public enum LocalConnectorApprovalMonitoringPolicy {
    public static func consistencyCheckInterval(
        hasStreamingService: Bool
    ) -> Duration {
        hasStreamingService ? .seconds(60) : .seconds(2)
    }
}

/// Application use case for approval settings, durable pending work, and decisions.
public struct LocalConnectorApprovalUseCase: Sendable {
    private let service: any LocalConnectorApprovalServicing

    public init(service: any LocalConnectorApprovalServicing) {
        self.service = service
    }

    public func loadSnapshot() async throws -> LocalConnectorApprovalSnapshot {
        async let settings = service.fetchApprovalSettings()
        async let pending = service.fetchPendingApprovals()
        return try await LocalConnectorApprovalSnapshot(
            settings: settings,
            pendingApprovals: pending
        )
    }

    public func refreshPendingApprovals() async throws -> [LocalConnectorPendingApproval] {
        try await service.fetchPendingApprovals()
    }

    public func updateDefaultMode(
        _ mode: LocalConnectorApprovalMode,
        riskAcknowledged: Bool
    ) async throws -> LocalConnectorApprovalSettings {
        try await service.updateDefaultApprovalMode(
            mode,
            riskAcknowledged: riskAcknowledged
        )
    }

    public func resolve(
        id: String,
        decision: String
    ) async throws -> LocalConnectorApprovalSnapshot {
        try await service.resolveApproval(id: id, decision: decision)
        try Task.checkCancellation()
        return try await loadSnapshot()
    }
}
