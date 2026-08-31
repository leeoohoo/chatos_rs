import CryptoKit
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeMCPCodeWriteToolsTests {
    @Test
    func stagesAndCommitsMultipleFilesAsOneSession() async throws {
        let fixture = try WriteFixture()
        defer { fixture.dispose() }
        let existing = fixture.root.appendingPathComponent("existing.txt")
        try Data("before\n".utf8).write(to: existing)
        let store = NativeMCPCodeWriteStore()
        let scope = fixture.scope

        let opened = try await store.call(
            name: "open_edit_session",
            arguments: [:],
            scope: scope,
            projectRoot: fixture.root
        )
        let sessionID = try opened.string(at: ["result", "session_id"])
        _ = try await store.call(
            name: "stage_edit_batch",
            arguments: [
                "session_id": .string(sessionID),
                "operations": .array([
                    .object([
                        "kind": .string("replace_text"),
                        "path": .string("existing.txt"),
                        "old_text": .string("before"),
                        "new_text": .string("after"),
                        "expected_sha256": .string(Self.sha256("before\n")),
                    ]),
                    .object([
                        "kind": .string("write"),
                        "path": .string("nested/new.txt"),
                        "content": .string("created\n"),
                        "expected_sha256": .null,
                    ]),
                ]),
            ],
            scope: scope,
            projectRoot: fixture.root
        )
        let committed = try await store.call(
            name: "commit_edit_session",
            arguments: ["session_id": .string(sessionID)],
            scope: scope,
            projectRoot: fixture.root
        )

        #expect(try committed.bool(at: ["changed"]) == true)
        #expect(try String(contentsOf: existing, encoding: .utf8) == "after\n")
        #expect(try String(contentsOf: fixture.root.appendingPathComponent("nested/new.txt"), encoding: .utf8) == "created\n")
    }

    @Test
    func refusesCommitWhenFileChangedAfterStaging() async throws {
        let fixture = try WriteFixture()
        defer { fixture.dispose() }
        let file = fixture.root.appendingPathComponent("shared.txt")
        try Data("baseline".utf8).write(to: file)
        let store = NativeMCPCodeWriteStore()
        let opened = try await store.call(
            name: "open_edit_session",
            arguments: [:],
            scope: fixture.scope,
            projectRoot: fixture.root
        )
        let sessionID = try opened.string(at: ["result", "session_id"])
        _ = try await store.call(
            name: "stage_edit_batch",
            arguments: [
                "session_id": .string(sessionID),
                "operations": .array([.object([
                    "kind": .string("write"),
                    "path": .string("shared.txt"),
                    "content": .string("agent change"),
                    "expected_sha256": .string(Self.sha256("baseline")),
                ])]),
            ],
            scope: fixture.scope,
            projectRoot: fixture.root
        )
        try Data("user change".utf8).write(to: file, options: .atomic)

        await #expect(throws: NativeMCPCodeWriteError.self) {
            _ = try await store.call(
                name: "commit_edit_session",
                arguments: ["session_id": .string(sessionID)],
                scope: fixture.scope,
                projectRoot: fixture.root
            )
        }
        #expect(try String(contentsOf: file, encoding: .utf8) == "user change")
    }

    private static func sha256(_ value: String) -> String {
        SHA256.hash(data: Data(value.utf8)).map { String(format: "%02x", $0) }.joined()
    }
}

private struct WriteFixture {
    let root: URL
    let scope = NativeMCPCodeWriteScope(
        workspaceID: "workspace-test",
        sessionID: "session-test",
        runID: "run-test"
    )

    init() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-write-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    func dispose() { try? FileManager.default.removeItem(at: root) }
}

private extension NativeJSONValue {
    func string(at path: [String]) throws -> String {
        var value = self
        for key in path {
            guard case let .object(values) = value, let next = values[key] else {
                throw WriteTestError.invalidShape
            }
            value = next
        }
        guard case let .string(result) = value else { throw WriteTestError.invalidShape }
        return result
    }

    func bool(at path: [String]) throws -> Bool {
        var value = self
        for key in path {
            guard case let .object(values) = value, let next = values[key] else {
                throw WriteTestError.invalidShape
            }
            value = next
        }
        guard case let .bool(result) = value else { throw WriteTestError.invalidShape }
        return result
    }
}

private enum WriteTestError: Error { case invalidShape }
