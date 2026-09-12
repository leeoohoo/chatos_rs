// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
@testable import ChatOSConnector
import Testing

struct NativeLocalAgentSnapshotBuilderTests {
    @Test("canonical digest matches the Rust protocol fixture")
    func matchesRustCanonicalDigest() throws {
        let payload: LocalAgentJSONValue = .object([
            "unicode": .string("设计/AI"),
            "prompt_revision": .string("main-prompt-1"),
            "array": .array([
                .object(["z": .signed(2), "a": .signed(1)]),
                .bool(true),
                .null,
            ]),
        ])

        let snapshot = try NativeLocalAgentSnapshotBuilder.make(
            snapshotID: "main-chat-prompt",
            revision: "main-prompt-1",
            payload: payload
        )

        #expect(
            snapshot.digest
                == "sha256:5fdb7990d695ddaf6dd10c66b4535d905e66344496d1e1673ae9c8148a998455"
        )
    }

    @Test("object key order cannot change a snapshot digest")
    func recursivelySortsObjectKeys() throws {
        let first = try NativeLocalAgentSnapshotBuilder.make(
            snapshotID: "snapshot-1",
            revision: "revision-1",
            payload: .object([
                "b": .signed(2),
                "nested": .object(["z": .bool(true), "a": .signed(1)]),
            ])
        )
        let second = try NativeLocalAgentSnapshotBuilder.make(
            snapshotID: "snapshot-1",
            revision: "revision-1",
            payload: .object([
                "nested": .object(["a": .signed(1), "z": .bool(true)]),
                "b": .signed(2),
            ])
        )

        #expect(first.digest == second.digest)
    }

    @Test("rejects non-object payloads and unsafe identities")
    func rejectsInvalidSnapshots() {
        #expect(throws: NativeLocalAgentSnapshotBuilderError.payloadMustBeObject) {
            _ = try NativeLocalAgentSnapshotBuilder.make(
                snapshotID: "snapshot-1",
                revision: "revision-1",
                payload: .array([])
            )
        }
        #expect(throws: NativeLocalAgentSnapshotBuilderError.invalidIdentity) {
            _ = try NativeLocalAgentSnapshotBuilder.make(
                snapshotID: " snapshot-1",
                revision: "revision-1",
                payload: .object([:])
            )
        }
    }
}
