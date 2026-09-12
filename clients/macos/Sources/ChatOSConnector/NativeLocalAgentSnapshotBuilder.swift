// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public enum NativeLocalAgentSnapshotBuilderError: Error, Equatable, Sendable {
    case invalidIdentity
    case payloadMustBeObject
}

extension NativeLocalAgentSnapshotBuilderError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .invalidIdentity:
            "本地 Agent 冻结快照身份无效"
        case .payloadMustBeObject:
            "本地 Agent 冻结快照内容必须是对象"
        }
    }
}

/// The single native constructor for every immutable Local Agent snapshot.
/// JSONEncoder's sortedKeys option applies recursively, matching the Rust
/// protocol's canonical object-key ordering. Both sides hash the exact UTF-8
/// JSON bytes and include the `sha256:` algorithm prefix on the wire.
public enum NativeLocalAgentSnapshotBuilder {
    public static func make(
        snapshotID: String,
        revision: String,
        payload: LocalAgentJSONValue
    ) throws -> LocalAgentFrozenSnapshot {
        guard validIdentity(snapshotID), validIdentity(revision) else {
            throw NativeLocalAgentSnapshotBuilderError.invalidIdentity
        }
        guard case .object = payload else {
            throw NativeLocalAgentSnapshotBuilderError.payloadMustBeObject
        }
        return LocalAgentFrozenSnapshot(
            snapshotID: snapshotID,
            revision: revision,
            digest: "sha256:\(try NativePluginHash.localAgentCanonicalSHA256(payload))",
            payload: payload
        )
    }

    private static func validIdentity(_ value: String) -> Bool {
        !value.isEmpty
            && value.count <= 512
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }
}
