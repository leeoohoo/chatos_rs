import Foundation
import Testing
@testable import ChatOSConnector

struct NativeMCPTerminalStoreTests {
    @Test
    func backgroundProcessCanBePolledAndWaited() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeMCPTerminalStore()
        let started = try await store.execute(
            command: "printf 'first\\n'; sleep 0.1; printf 'second\\n'",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: true
        )
        let id = try started.string("terminal_id")
        let waited = try await store.call(
            name: "process_wait",
            arguments: [
                "terminal_id": .string(id),
                "timeout_ms": .number(5_000),
            ],
            projectRoot: fixture.root
        )
        let output = try waited.string("output")

        #expect(output.contains("first"))
        #expect(output.contains("second"))
        #expect(try waited.bool("exited"))
    }

    @Test
    func stdinCanBeWrittenToRunningProcess() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeMCPTerminalStore()
        let started = try await store.execute(
            command: "read value; printf 'received:%s\\n' \"$value\"",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: true
        )
        let id = try started.string("terminal_id")
        _ = try await store.call(
            name: "process_write",
            arguments: [
                "terminal_id": .string(id),
                "data": .string("hello"),
                "submit": .bool(true),
            ],
            projectRoot: fixture.root
        )
        let waited = try await store.call(
            name: "process_wait",
            arguments: [
                "terminal_id": .string(id),
                "timeout_ms": .number(5_000),
            ],
            projectRoot: fixture.root
        )

        #expect(try waited.string("output").contains("received:hello"))
    }
}

private struct TerminalFixture {
    let root: URL

    init() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-terminal-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    func dispose() { try? FileManager.default.removeItem(at: root) }
}

private extension NativeJSONValue {
    func string(_ key: String) throws -> String {
        guard case let .object(values) = self, case let .string(value)? = values[key] else {
            throw TerminalTestError.invalidShape
        }
        return value
    }

    func bool(_ key: String) throws -> Bool {
        guard case let .object(values) = self, case let .bool(value)? = values[key] else {
            throw TerminalTestError.invalidShape
        }
        return value
    }
}

private enum TerminalTestError: Error { case invalidShape }
