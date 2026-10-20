import Testing
@testable import ChatOSCore

@Suite("Local Agent service shutdown")
struct LocalAgentServiceShutdownUseCaseTests {
    private actor RecordingPort: LocalAgentServiceShutdownPort {
        private var stages: [LocalAgentServiceShutdownStage] = []

        func reset(_ stage: LocalAgentServiceShutdownStage) {
            stages.append(stage)
        }

        func snapshot() -> [LocalAgentServiceShutdownStage] {
            stages
        }
    }

    @Test
    func resetsEveryServiceInTheExistingMacOSOrderAndStopsHostLast() async {
        let port = RecordingPort()

        await LocalAgentServiceShutdownUseCase().execute(port: port)

        let stages = await port.snapshot()
        #expect(stages == LocalAgentServiceShutdownStage.allCases)
        #expect(stages.last == .host)
    }
}
