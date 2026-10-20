import Foundation
import Testing
@testable import ChatOSCore

@Suite("Local Agent control-plane bootstrap use case")
struct LocalAgentControlPlaneBootstrapUseCaseTests {
    private enum RetryableError: Error {
        case failed
    }

    private struct DefinitiveError: LocalAgentControlPlaneBootstrapNonRetryableError {}

    @MainActor
    private final class RecordingPort: LocalAgentControlPlaneBootstrapPort {
        var attempts = 0
        var failuresBeforeSuccess = 0
        var error: Error = RetryableError.failed

        func bootstrap(ownerUserID: String) throws {
            attempts += 1
            if attempts <= failuresBeforeSuccess {
                throw error
            }
        }
    }

    private actor DelayRecorder {
        private var delays: [Duration] = []

        func append(_ delay: Duration) {
            delays.append(delay)
        }

        func snapshot() -> [Duration] {
            delays
        }
    }

    @Test @MainActor
    func retryableFailuresUseTheBoundedDelaySequence() async throws {
        let port = RecordingPort()
        port.failuresBeforeSuccess = 2
        let recorder = DelayRecorder()
        let useCase = LocalAgentControlPlaneBootstrapUseCase(
            retryDelays: [nil, .seconds(1), .seconds(2)],
            sleep: { await recorder.append($0) }
        )

        try await useCase.execute(
            ownerUserID: "owner-1",
            port: port,
            isCurrent: { true }
        )

        #expect(port.attempts == 3)
        #expect(await recorder.snapshot() == [.seconds(1), .seconds(2)])
    }

    @Test @MainActor
    func definitiveFailureDoesNotRetry() async {
        let port = RecordingPort()
        port.failuresBeforeSuccess = 3
        port.error = DefinitiveError()
        let useCase = LocalAgentControlPlaneBootstrapUseCase(
            retryDelays: [nil, .seconds(1), .seconds(2)],
            sleep: { _ in }
        )

        await #expect(throws: DefinitiveError.self) {
            try await useCase.execute(
                ownerUserID: "owner-1",
                port: port,
                isCurrent: { true }
            )
        }

        #expect(port.attempts == 1)
    }

    @Test @MainActor
    func staleLifecycleCannotBeginTheNextAttempt() async {
        let port = RecordingPort()
        port.failuresBeforeSuccess = 3
        var checks = 0
        let useCase = LocalAgentControlPlaneBootstrapUseCase(
            retryDelays: [nil, nil, nil],
            sleep: { _ in }
        )

        await #expect(throws: CancellationError.self) {
            try await useCase.execute(
                ownerUserID: "owner-1",
                port: port,
                isCurrent: {
                    checks += 1
                    return checks == 1
                }
            )
        }

        #expect(port.attempts == 1)
    }

    @Test @MainActor
    func lastRetryableFailureIsReturned() async {
        let port = RecordingPort()
        port.failuresBeforeSuccess = 3
        let useCase = LocalAgentControlPlaneBootstrapUseCase(
            retryDelays: [nil, nil, nil],
            sleep: { _ in }
        )

        await #expect(throws: RetryableError.failed) {
            try await useCase.execute(
                ownerUserID: "owner-1",
                port: port,
                isCurrent: { true }
            )
        }

        #expect(port.attempts == 3)
    }
}
