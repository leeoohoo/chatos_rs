import Testing
@testable import ChatOSCore

@Suite("Local Agent service configuration")
struct LocalAgentServiceConfigurationUseCaseTests {
    private enum TestError: Error {
        case failed
    }

    private actor RecordingPort: LocalAgentServiceConfigurationPort {
        private var stages: [LocalAgentServiceConfigurationStage] = []
        private let failingStage: LocalAgentServiceConfigurationStage?

        init(failingStage: LocalAgentServiceConfigurationStage? = nil) {
            self.failingStage = failingStage
        }

        func configure(_ stage: LocalAgentServiceConfigurationStage) async throws {
            stages.append(stage)
            if stage == failingStage {
                throw TestError.failed
            }
        }

        func snapshot() -> [LocalAgentServiceConfigurationStage] {
            stages
        }
    }

    @Test @MainActor
    func configuresEveryServiceInTheExistingMacOSOrder() async throws {
        let port = RecordingPort()

        try await LocalAgentServiceConfigurationUseCase().execute(
            port: port,
            isCurrent: { true }
        )

        #expect(await port.snapshot() == LocalAgentServiceConfigurationStage.allCases)
    }

    @Test @MainActor
    func staleLifecycleStopsAfterTheCurrentAwait() async {
        let port = RecordingPort()
        var acceptanceChecks = 0

        await #expect(throws: CancellationError.self) {
            try await LocalAgentServiceConfigurationUseCase().execute(
                port: port,
                isCurrent: {
                    acceptanceChecks += 1
                    return acceptanceChecks == 1
                }
            )
        }

        #expect(await port.snapshot() == [.localApprovalAgent])
    }

    @Test @MainActor
    func adapterFailureStopsLaterConfigurationStages() async {
        let port = RecordingPort(failingStage: .petActivity)

        await #expect(throws: TestError.failed) {
            try await LocalAgentServiceConfigurationUseCase().execute(
                port: port,
                isCurrent: { true }
            )
        }

        #expect(await port.snapshot() == [
            .localApprovalAgent,
            .runtimeSettings,
            .petActivity,
        ])
    }
}
