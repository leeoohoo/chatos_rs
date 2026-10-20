/// Ordered teardown stages for one Local Agent runtime.
///
/// The Host is always stopped last. This preserves the existing macOS behavior: adapters first
/// discard account-scoped state, then the process transport is terminated.
public enum LocalAgentServiceShutdownStage: String, CaseIterable, Sendable, Equatable {
    case conversationCommand
    case petActivity
    case messageTaskGraph
    case askUserPrompt
    case turnProcess
    case runtimeSettings
    case platformToolWorker
    case workspace
    case projectConversation
    case notepad
    case remoteConnectionMetadata
    case host
}

public protocol LocalAgentServiceShutdownPort: Sendable {
    func reset(_ stage: LocalAgentServiceShutdownStage) async
}

/// Tears down Host-backed services in their established macOS order.
///
/// Shutdown deliberately runs every stage without a lifecycle acceptance gate: it operates on a
/// frozen adapter set, and the composition root serializes it ahead of the next Host launch.
public struct LocalAgentServiceShutdownUseCase: Sendable {
    public init() {}

    public func execute(port: any LocalAgentServiceShutdownPort) async {
        for stage in LocalAgentServiceShutdownStage.allCases {
            await port.reset(stage)
        }
    }
}
