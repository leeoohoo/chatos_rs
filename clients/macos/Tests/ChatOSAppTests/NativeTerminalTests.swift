import Foundation
import Testing
import ChatOSCore
@testable import ChatOSApp

@Suite("Native macOS terminal")
@MainActor
struct NativeTerminalTests {
    @Test("closing a terminal is idempotent")
    func closeIsIdempotent() {
        let terminal = NativeLocalTerminalViewModel(
            workingDirectory: "/tmp",
            useCase: makeUseCase()
        )

        terminal.close()
        terminal.close()

        #expect(terminal.state == .closed)
    }

    @Test("workspace closes the PTY before replacing its last tab")
    func workspaceClosesPTYBeforeReplacingLastTab() {
        let workspace = TerminalWorkspaceViewModel(
            workingDirectory: "/tmp",
            useCase: makeUseCase()
        )
        let original = workspace.sessions[0]

        workspace.closeTerminal(id: original.id)

        #expect(original.terminal.state == .closed)
        #expect(workspace.sessions.count == 1)
        #expect(workspace.sessions[0].id != original.id)
    }

    @Test("workspace can release all terminals and lazily create a new tab")
    func workspaceApplicationLifecycle() {
        let workspace = TerminalWorkspaceViewModel(
            workingDirectory: "/tmp",
            useCase: makeUseCase()
        )
        let original = workspace.sessions[0]

        workspace.closeAllTerminals()

        #expect(original.terminal.state == .closed)
        #expect(workspace.sessions.isEmpty)
        #expect(workspace.selectedSessionID == nil)

        workspace.ensureTerminal()
        #expect(workspace.sessions.count == 1)
        #expect(workspace.sessions[0].id != original.id)
    }

    @Test("terminal renders byte-stream output and applies termination")
    func localTerminalByteStream() async throws {
        let marker = "__CHATOS_NATIVE_PTY_\(UUID().uuidString)__"
        let session = LocalTerminalSessionStub()
        let terminal = NativeLocalTerminalViewModel(
            workingDirectory: "/tmp",
            useCase: makeUseCase(session: session)
        )
        defer { terminal.close() }

        terminal.startIfNeeded()
        #expect(terminal.state == .running)
        session.emit(.output(Data("\(marker)\n".utf8)))
        try await waitUntil(timeout: .seconds(5)) {
            String(
                decoding: terminal.terminalView.terminal.getBufferAsData(),
                as: UTF8.self
            )
            .replacingOccurrences(of: "\r", with: "")
            .replacingOccurrences(of: "\n", with: "")
            .contains(marker)
        }
        session.emit(.terminated(exitCode: 0))
        try await waitUntil(timeout: .seconds(5)) {
            terminal.state == .exited(0)
        }
    }

    private func makeUseCase(
        session: LocalTerminalSessionStub = .init()
    ) -> LocalTerminalUseCase {
        LocalTerminalUseCase(provider: LocalTerminalProviderStub(session: session))
    }

    private func waitUntil(
        timeout: Duration,
        condition: @escaping @MainActor () -> Bool
    ) async throws {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: timeout)
        while !condition() {
            if clock.now >= deadline {
                Issue.record("Timed out waiting for the terminal process")
                return
            }
            try await Task.sleep(for: .milliseconds(20))
        }
    }
}

private struct LocalTerminalProviderStub: LocalTerminalSessionProviding {
    let session: LocalTerminalSessionStub

    func makeLocalTerminalSession(
        workingDirectory: String
    ) throws -> any LocalTerminalSessionPort {
        session
    }
}

private final class LocalTerminalSessionStub: LocalTerminalSessionPort,
    @unchecked Sendable {
    let shellName = "test-shell"
    private(set) var isRunning = false
    private let stream: AsyncStream<LocalTerminalEvent>
    private let continuation: AsyncStream<LocalTerminalEvent>.Continuation

    init() {
        let pair = AsyncStream<LocalTerminalEvent>.makeStream()
        stream = pair.stream
        continuation = pair.continuation
    }

    func events() -> AsyncStream<LocalTerminalEvent> { stream }
    func start(columns: Int, rows: Int) { isRunning = true }
    func send(_ data: Data) {}
    func resize(columns: Int, rows: Int) {}
    func interrupt() {}

    func close() {
        isRunning = false
        continuation.finish()
    }

    func emit(_ event: LocalTerminalEvent) {
        continuation.yield(event)
        if case .terminated = event {
            isRunning = false
            continuation.finish()
        }
    }
}
