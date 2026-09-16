import Foundation
import Testing
@testable import ChatOSApp

@Suite("Native macOS terminal")
@MainActor
struct NativeTerminalTests {
    @Test("closing a terminal is idempotent")
    func closeIsIdempotent() {
        let terminal = NativeLocalTerminalViewModel(workingDirectory: "/tmp")

        terminal.close()
        terminal.close()

        #expect(terminal.state == .closed)
    }

    @Test("workspace closes the PTY before replacing its last tab")
    func workspaceClosesPTYBeforeReplacingLastTab() {
        let workspace = TerminalWorkspaceViewModel(workingDirectory: "/tmp")
        let original = workspace.sessions[0]

        workspace.closeTerminal(id: original.id)

        #expect(original.terminal.state == .closed)
        #expect(workspace.sessions.count == 1)
        #expect(workspace.sessions[0].id != original.id)
    }

    @Test("workspace can release all terminals and lazily create a new tab")
    func workspaceApplicationLifecycle() {
        let workspace = TerminalWorkspaceViewModel(workingDirectory: "/tmp")
        let original = workspace.sessions[0]

        workspace.closeAllTerminals()

        #expect(original.terminal.state == .closed)
        #expect(workspace.sessions.isEmpty)
        #expect(workspace.selectedSessionID == nil)

        workspace.ensureTerminal()
        #expect(workspace.sessions.count == 1)
        #expect(workspace.sessions[0].id != original.id)
    }

    @Test("login shell accepts streaming input through its PTY")
    func localPTYSmokeTest() async throws {
        let marker = "__CHATOS_NATIVE_PTY_\(UUID().uuidString)__"
        let terminal = NativeLocalTerminalViewModel(workingDirectory: "/tmp")
        defer { terminal.close() }

        terminal.startIfNeeded()
        #expect(terminal.state == .running)

        terminal.terminalView.send(txt: "printf '\(marker)|%s|%s\\n' \"$TERM\" \"$COLORTERM\"; exit\n")

        try await waitUntil(timeout: .seconds(5)) {
            if case .exited = terminal.state { return true }
            return false
        }
        let contents = String(
            decoding: terminal.terminalView.terminal.getBufferAsData(),
            as: UTF8.self
        )
        let unwrappedContents = contents
            .replacingOccurrences(of: "\r", with: "")
            .replacingOccurrences(of: "\n", with: "")
        #expect(unwrappedContents.contains("\(marker)|xterm-256color|truecolor"))
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
