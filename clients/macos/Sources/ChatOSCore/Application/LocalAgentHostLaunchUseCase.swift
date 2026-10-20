import Foundation

public enum LocalAgentHostLaunchTransition: Sendable, Equatable {
    case handshaking
    case synchronizing
    case restoredServicesReady
    case workspaceReady
}

@MainActor
public protocol LocalAgentHostLaunchPort {
    func resetEventHub() async
    func startHost(ownerUserID: String) async throws
    func restoreLocalServices(ownerUserID: String) async throws -> Bool
    func configureWorkspace(ownerUserID: String) async
    func configureProjectConversation(ownerUserID: String) async
    func configureNotepad(ownerUserID: String) async
    func configureRemoteConnectionMetadata(ownerUserID: String) async
}

/// Application orchestration for the local-first Host launch path.
///
/// Authenticated gateway bootstrap remains a separate lifecycle because it has its own retries and
/// request token. This use case starts the bootstrap Host, restores any local control-plane state,
/// and configures the workspace-facing services in the exact order used by the macOS client.
public struct LocalAgentHostLaunchUseCase: Sendable {
    public init() {}

    @MainActor
    public func execute(
        ownerUserID: String,
        port: any LocalAgentHostLaunchPort,
        isCurrent: () -> Bool,
        onTransition: (LocalAgentHostLaunchTransition) throws -> Void
    ) async throws {
        try validate(isCurrent)
        await port.resetEventHub()
        try validate(isCurrent)

        try onTransition(.handshaking)
        try await port.startHost(ownerUserID: ownerUserID)
        try validate(isCurrent)

        try onTransition(.synchronizing)
        do {
            let didRestoreServices = try await port.restoreLocalServices(ownerUserID: ownerUserID)
            try validate(isCurrent)
            if didRestoreServices {
                try onTransition(.restoredServicesReady)
                try validate(isCurrent)
            }
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            // A missing or stale local snapshot does not block workspace setup or the later
            // authenticated gateway bootstrap from repairing the control plane.
            try validate(isCurrent)
        }

        await port.configureWorkspace(ownerUserID: ownerUserID)
        try validate(isCurrent)
        await port.configureProjectConversation(ownerUserID: ownerUserID)
        try validate(isCurrent)
        try onTransition(.workspaceReady)
        try validate(isCurrent)

        await port.configureNotepad(ownerUserID: ownerUserID)
        try validate(isCurrent)
        await port.configureRemoteConnectionMetadata(ownerUserID: ownerUserID)
        try validate(isCurrent)
    }

    private func validate(_ isCurrent: () -> Bool) throws {
        guard !Task.isCancelled, isCurrent() else { throw CancellationError() }
    }
}
