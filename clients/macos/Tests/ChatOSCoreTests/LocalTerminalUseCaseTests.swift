import Foundation
import Testing
@testable import ChatOSCore

@Suite("Local terminal application boundary")
struct LocalTerminalUseCaseTests {
    @Test
    func useCaseRoutesWorkingDirectoryThroughProvider() throws {
        let provider = LocalTerminalProviderSpy()
        let useCase = LocalTerminalUseCase(provider: provider)

        let session = try useCase.open(workingDirectory: "/workspace/project")

        #expect(session.shellName == "test-shell")
        #expect(provider.workingDirectories == ["/workspace/project"])
    }

    @Test
    func replacingAndResettingSessionRejectsOlderEvents() {
        var state = LocalTerminalLifecycleState()
        let terminalID = UUID()
        let old = state.begin(terminalID: terminalID)
        let current = state.begin(terminalID: terminalID)

        #expect(!state.accepts(old))
        #expect(state.accepts(current))
        let finishedOld = state.finish(old)
        let finishedCurrent = state.finish(current)
        #expect(!finishedOld)
        #expect(finishedCurrent)

        let replacement = state.begin(terminalID: terminalID)
        state.reset()
        #expect(!state.accepts(replacement))
    }
}

private final class LocalTerminalProviderSpy: LocalTerminalSessionProviding,
    @unchecked Sendable {
    private(set) var workingDirectories: [String] = []

    func makeLocalTerminalSession(
        workingDirectory: String
    ) throws -> any LocalTerminalSessionPort {
        workingDirectories.append(workingDirectory)
        return LocalTerminalCoreSessionStub()
    }
}

private final class LocalTerminalCoreSessionStub: LocalTerminalSessionPort,
    @unchecked Sendable {
    let shellName = "test-shell"
    var isRunning = false

    func events() -> AsyncStream<LocalTerminalEvent> {
        AsyncStream { $0.finish() }
    }

    func start(columns: Int, rows: Int) { isRunning = true }
    func send(_ data: Data) {}
    func resize(columns: Int, rows: Int) {}
    func interrupt() {}
    func close() { isRunning = false }
}
