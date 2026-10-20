import Foundation

/// Ordered service configuration stages required before the Local Agent runtime is ready.
///
/// The order mirrors the existing macOS bootstrap flow. Platform adapters decide how each stage
/// is implemented; the Application layer owns sequencing and lifecycle validation.
public enum LocalAgentServiceConfigurationStage: String, CaseIterable, Sendable, Equatable {
    case localApprovalAgent
    case runtimeSettings
    case petActivity
    case conversationCommand
    case messageTaskGraph
    case askUserPrompt
    case turnProcess
    case platformToolWorker
}

public protocol LocalAgentServiceConfigurationPort: Sendable {
    func configure(_ stage: LocalAgentServiceConfigurationStage) async throws
}

/// Configures every Host-backed application service for one authenticated runtime lifecycle.
///
/// The caller supplies the lifecycle acceptance check because account and Host generations are
/// owned by the composition root. It is checked before the first stage and after every await, so
/// an adapter that ignores cancellation still cannot continue configuring a stale lifecycle.
public struct LocalAgentServiceConfigurationUseCase: Sendable {
    public init() {}

    @MainActor
    public func execute(
        port: any LocalAgentServiceConfigurationPort,
        isCurrent: () -> Bool
    ) async throws {
        guard isCurrent() else { throw CancellationError() }
        for stage in LocalAgentServiceConfigurationStage.allCases {
            try await port.configure(stage)
            guard isCurrent() else { throw CancellationError() }
        }
    }
}
