import Foundation

/// Marker for bootstrap failures that are definitive and must not be retried automatically.
public protocol LocalAgentControlPlaneBootstrapNonRetryableError: Error {}

@MainActor
public protocol LocalAgentControlPlaneBootstrapPort {
    func bootstrap(ownerUserID: String) async throws
}

/// Retries authenticated control-plane bootstrap with the existing bounded macOS policy.
public struct LocalAgentControlPlaneBootstrapUseCase: Sendable {
    private let retryDelays: [Duration?]
    private let sleep: @Sendable (Duration) async throws -> Void

    public init() {
        retryDelays = [
            nil,
            .seconds(1),
            .seconds(2),
            .seconds(5),
            .seconds(10),
        ]
        sleep = { delay in try await Task.sleep(for: delay) }
    }

    init(
        retryDelays: [Duration?],
        sleep: @escaping @Sendable (Duration) async throws -> Void
    ) {
        precondition(!retryDelays.isEmpty)
        self.retryDelays = retryDelays
        self.sleep = sleep
    }

    @MainActor
    public func execute(
        ownerUserID: String,
        port: any LocalAgentControlPlaneBootstrapPort,
        isCurrent: () -> Bool
    ) async throws {
        for (attempt, retryDelay) in retryDelays.enumerated() {
            if let retryDelay {
                try await sleep(retryDelay)
            }
            try validate(isCurrent)
            do {
                try await port.bootstrap(ownerUserID: ownerUserID)
                try validate(isCurrent)
                return
            } catch is CancellationError {
                throw CancellationError()
            } catch let error as any LocalAgentControlPlaneBootstrapNonRetryableError {
                throw error
            } catch {
                try validate(isCurrent)
                guard attempt == retryDelays.indices.last else { continue }
                throw error
            }
        }
    }

    private func validate(_ isCurrent: () -> Bool) throws {
        guard !Task.isCancelled, isCurrent() else { throw CancellationError() }
    }
}
