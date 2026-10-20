import Foundation
import Testing
@testable import ChatOSConnector
import ChatOSCore

@Suite("Native local terminal session")
struct NativeLocalTerminalSessionTests {
    @Test
    func loginShellStreamsOutputThroughCorePort() async throws {
        let marker = "__CHATOS_LOCAL_TERMINAL_\(UUID().uuidString)__"
        let session = try NativeLocalTerminalSession(workingDirectory: "/tmp")
        defer { session.close() }
        let events = session.events()

        session.start(columns: 80, rows: 24)
        #expect(session.isRunning)
        session.send(Data("printf '\(marker)|%s|%s\\n' \"$TERM\" \"$COLORTERM\"; exit\n".utf8))

        let received = try await withThrowingTaskGroup(of: Bool.self) { group in
            group.addTask {
                var output = Data()
                for await event in events {
                    switch event {
                    case let .output(data):
                        output.append(data)
                        if String(decoding: output, as: UTF8.self).contains(marker) {
                            return true
                        }
                    case .terminated:
                        return String(decoding: output, as: UTF8.self).contains(marker)
                    case .failed:
                        return false
                    }
                }
                return false
            }
            group.addTask {
                try await Task.sleep(for: .seconds(5))
                return false
            }
            let first = try await group.next() ?? false
            group.cancelAll()
            return first
        }
        #expect(received)
    }

    @Test
    func invalidWorkingDirectoryFailsBeforeProcessLaunch() {
        let missing = "/tmp/chatos-missing-\(UUID().uuidString)"
        #expect(throws: NativeLocalTerminalError.invalidWorkingDirectory) {
            try NativeLocalTerminalSession(workingDirectory: missing)
        }
    }

    @Test
    func outputBacklogIsBoundedAndFailsClosed() async throws {
        let session = try NativeLocalTerminalSession(workingDirectory: "/tmp")
        let events = session.events()

        for _ in 0..<257 {
            session.dataReceived(slice: [UInt8](repeating: 65, count: 1)[...])
        }

        var failure: String?
        for await event in events {
            if case let .failed(message) = event {
                failure = message
            }
        }
        #expect(failure != nil)
        #expect(!session.isRunning)
    }
}
