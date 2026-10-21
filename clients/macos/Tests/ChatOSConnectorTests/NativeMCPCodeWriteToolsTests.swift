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

    @Test
    func discardingExecutorRunRemovesUncommittedEditSession() async throws {
        let fixture = try WriteFixture()
        defer { fixture.dispose() }
        let file = fixture.root.appendingPathComponent("pending.txt")
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
                    "path": .string("pending.txt"),
                    "content": .string("must not commit"),
                    "expected_sha256": .string(Self.sha256("baseline")),
                ])]),
            ],
            scope: fixture.scope,
            projectRoot: fixture.root
        )

        let discarded = await store.discard(runID: fixture.scope.runID)

        #expect(discarded == 1)
        await #expect(throws: NativeMCPCodeWriteError.self) {
            _ = try await store.call(
                name: "commit_edit_session",
                arguments: ["session_id": .string(sessionID)],
                scope: fixture.scope,
                projectRoot: fixture.root
            )
        }
        #expect(try String(contentsOf: file, encoding: .utf8) == "baseline")
    }

    @Test
    func sameRunResumeReusesStagedEditSession() async throws {
        let fixture = try WriteFixture()
        defer { fixture.dispose() }
        let file = fixture.root.appendingPathComponent("resume.txt")
        try Data("before".utf8).write(to: file)
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
                    "path": .string("resume.txt"),
                    "content": .string("after"),
                    "expected_sha256": .string(Self.sha256("before")),
                ])]),
            ],
            scope: fixture.scope,
            projectRoot: fixture.root
        )

        // A model attempt can end here. Reopening with the same immutable run scope must recover
        // the staged transaction rather than starting an empty session.
        let resumed = try await store.call(
            name: "open_edit_session",
            arguments: [:],
            scope: fixture.scope,
            projectRoot: fixture.root
        )
        #expect(try resumed.string(at: ["result", "session_id"]) == sessionID)
        #expect(try resumed.bool(at: ["result", "reused"]) == true)

        _ = try await store.call(
            name: "commit_edit_session",
            arguments: ["session_id": .string(sessionID)],
            scope: fixture.scope,
            projectRoot: fixture.root
        )
        #expect(try String(contentsOf: file, encoding: .utf8) == "after")
    }

    @Test
    func approvalEvidenceDescribesHostVerifiedStagedChangesForAIReview() async throws {
        let fixture = try WriteFixture()
        defer { fixture.dispose() }
        let sourceDirectory = fixture.root.appendingPathComponent("Sources", isDirectory: true)
        try FileManager.default.createDirectory(at: sourceDirectory, withIntermediateDirectories: true)
        let file = sourceDirectory.appendingPathComponent("Feature.swift")
        try Data("let value = 1\n".utf8).write(to: file)
        let store = NativeMCPCodeWriteStore()
        let ownedPaths = ["Sources"]
        let opened = try await store.call(
            name: "open_edit_session",
            arguments: ["purpose": .string("修复工具栏图标")],
            scope: fixture.scope,
            projectRoot: fixture.root,
            ownedPaths: ownedPaths
        )
        let sessionID = try opened.string(at: ["result", "session_id"])
        _ = try await store.call(
            name: "stage_edit_batch",
            arguments: [
                "session_id": .string(sessionID),
                "operations": .array([.object([
                    "kind": .string("write"),
                    "path": .string("Sources/Feature.swift"),
                    "content": .string("let value = 2\n"),
                    "expected_sha256": .string(Self.sha256("let value = 1\n")),
                ])]),
            ],
            scope: fixture.scope,
            projectRoot: fixture.root,
            ownedPaths: ownedPaths
        )

        let evidence = try await store.approvalEvidence(
            arguments: ["session_id": .string(sessionID)],
            scope: fixture.scope,
            projectRoot: fixture.root,
            ownedPaths: ownedPaths
        )

        #expect(evidence.purpose == "修复工具栏图标")
        #expect(evidence.evaluationSummary.contains("Sources/Feature.swift"))
        #expect(evidence.evaluationSummary.contains("Task owned_paths"))
        #expect(evidence.evaluationSummary.contains("1 行/14 字节 → 1 行/14 字节"))
        #expect(evidence.evaluationSummary.contains("EVIDENCE_COMPLETE=true"))
        #expect(evidence.evaluationSummary.contains("let value = 1"))
        #expect(evidence.evaluationSummary.contains("let value = 2"))
        #expect(try String(contentsOf: file, encoding: .utf8) == "let value = 1\n")
    }

    @Test
    func approvalEvidenceNeverSilentlyTruncatesOversizedStagedContent() async throws {
        let fixture = try WriteFixture()
        defer { fixture.dispose() }
        let file = fixture.root.appendingPathComponent("Feature.swift")
        try Data("let value = 1\n".utf8).write(to: file)
        let store = NativeMCPCodeWriteStore()
        let opened = try await store.call(
            name: "open_edit_session",
            arguments: [:],
            scope: fixture.scope,
            projectRoot: fixture.root,
            ownedPaths: ["Feature.swift"]
        )
        let sessionID = try opened.string(at: ["result", "session_id"])
        let tail = "COMPLETE_TAIL_MARKER"
        _ = try await store.call(
            name: "stage_edit_batch",
            arguments: [
                "session_id": .string(sessionID),
                "operations": .array([.object([
                    "kind": .string("write"),
                    "path": .string("Feature.swift"),
                    "content": .string(String(repeating: "x", count: 90_000) + tail),
                    "expected_sha256": .string(Self.sha256("let value = 1\n")),
                ])]),
            ],
            scope: fixture.scope,
            projectRoot: fixture.root,
            ownedPaths: ["Feature.swift"]
        )

        let evidence = try await store.approvalEvidence(
            arguments: ["session_id": .string(sessionID)],
            scope: fixture.scope,
            projectRoot: fixture.root,
            ownedPaths: ["Feature.swift"]
        )

        #expect(evidence.evaluationSummary.contains("EVIDENCE_COMPLETE=false"))
        #expect(evidence.evaluationSummary.contains("不得在证据不完整时猜测或放行"))
        #expect(!evidence.evaluationSummary.contains(tail))
        #expect(evidence.evaluationSummary.count <= 80_000)
    }

    @Test
    func approvalEvidenceWithEnvironmentVariantNeverSendsSecretContentToAI() async throws {
        let fixture = try WriteFixture()
        defer { fixture.dispose() }
        let file = fixture.root.appendingPathComponent(".env.staging")
        try Data("API_KEY=old-secret\n".utf8).write(to: file)
        let store = NativeMCPCodeWriteStore()
        let opened = try await store.call(
            name: "open_edit_session",
            arguments: [:],
            scope: fixture.scope,
            projectRoot: fixture.root,
            ownedPaths: [".env.staging"]
        )
        let sessionID = try opened.string(at: ["result", "session_id"])
        _ = try await store.call(
            name: "stage_edit_batch",
            arguments: [
                "session_id": .string(sessionID),
                "operations": .array([.object([
                    "kind": .string("write"),
                    "path": .string(".env.staging"),
                    "content": .string("API_KEY=new-secret\n"),
                    "expected_sha256": .string(Self.sha256("API_KEY=old-secret\n")),
                ])]),
            ],
            scope: fixture.scope,
            projectRoot: fixture.root,
            ownedPaths: [".env.staging"]
        )

        let evidence = try await store.approvalEvidence(
            arguments: ["session_id": .string(sessionID)],
            scope: fixture.scope,
            projectRoot: fixture.root,
            ownedPaths: [".env.staging"]
        )

        #expect(!evidence.isComplete)
        #expect(evidence.evaluationSummary.contains("EVIDENCE_COMPLETE=false"))
        #expect(!evidence.evaluationSummary.contains("old-secret"))
        #expect(!evidence.evaluationSummary.contains("new-secret"))
    }

    @Test
    func oversizedNumericEditCoordinatesAreRejectedWithoutIntegerTrap() async throws {
        let fixture = try WriteFixture()
        defer { fixture.dispose() }
        let file = fixture.root.appendingPathComponent("Feature.swift")
        try Data("let value = 1\n".utf8).write(to: file)
        let store = NativeMCPCodeWriteStore()
        let opened = try await store.call(
            name: "open_edit_session",
            arguments: [:],
            scope: fixture.scope,
            projectRoot: fixture.root,
            ownedPaths: ["Feature.swift"]
        )
        let sessionID = try opened.string(at: ["result", "session_id"])

        await #expect(throws: Error.self) {
            _ = try await store.call(
                name: "stage_edit_batch",
                arguments: [
                    "session_id": .string(sessionID),
                    "operations": .array([.object([
                        "kind": .string("replace_lines"),
                        "path": .string("Feature.swift"),
                        "start_line": .number(1e300),
                        "end_line": .number(1e300),
                        "content": .string("let value = 2"),
                    ])]),
                ],
                scope: fixture.scope,
                projectRoot: fixture.root,
                ownedPaths: ["Feature.swift"]
            )
        }
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
