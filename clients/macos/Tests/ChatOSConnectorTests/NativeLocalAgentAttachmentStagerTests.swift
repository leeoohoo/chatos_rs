// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
@testable import ChatOSConnector
import Foundation
import Testing

struct NativeLocalAgentAttachmentStagerTests {
    @Test("stages private payloads behind opaque integrity-bound grants")
    func stagesOpaqueGrants() throws {
        let root = temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let bytes = Data("visual design reference".utf8)
        let attachment = ConversationAttachmentDraft(
            id: "attachment-1",
            name: "reference.txt",
            mimeType: "text/plain",
            kind: .file,
            origin: .pastedText,
            data: bytes
        )

        let references = try NativeLocalAgentAttachmentStager().stage([attachment], in: root)

        let reference = try #require(references.first)
        #expect(reference.attachmentID == "attachment-1")
        #expect(reference.payloadReference.hasPrefix("attachment-grant:grant-"))
        #expect(reference.payloadDigest == "sha256:\(NativePluginHash.sha256(bytes))")
        #expect(reference.byteSize == UInt64(bytes.count))
        let grantID = String(reference.payloadReference.dropFirst("attachment-grant:".count))
        let payloadURL = root.appendingPathComponent("\(grantID).payload")
        #expect(try Data(contentsOf: payloadURL) == bytes)
        let attributes = try FileManager.default.attributesOfItem(atPath: payloadURL.path)
        #expect((attributes[.posixPermissions] as? NSNumber)?.intValue == 0o600)
    }

    @Test("rejects oversized and duplicate attachments before writing")
    func rejectsInvalidBatchesAtomically() throws {
        let root = temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let oversized = ConversationAttachmentDraft(
            id: "attachment-1",
            name: "large.bin",
            mimeType: "application/octet-stream",
            kind: .file,
            origin: .file,
            data: Data(count: NativeLocalAgentAttachmentStager.maximumAttachmentBytes + 1)
        )
        #expect(throws: NativeLocalAgentAttachmentStagingError.attachmentTooLarge("large.bin")) {
            _ = try NativeLocalAgentAttachmentStager().stage([oversized], in: root)
        }
        #expect(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)

        let valid = ConversationAttachmentDraft(
            id: "duplicate",
            name: "one.txt",
            mimeType: "text/plain",
            kind: .file,
            origin: .pastedText,
            data: Data("one".utf8)
        )
        #expect(throws: NativeLocalAgentAttachmentStagingError.invalidAttachment("one.txt")) {
            _ = try NativeLocalAgentAttachmentStager().stage([valid, valid], in: root)
        }
        #expect(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }

    @Test("discard removes only valid stager-owned grant payloads")
    func discardsOwnedGrants() throws {
        let root = temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let reference = try #require(NativeLocalAgentAttachmentStager().stage([
            ConversationAttachmentDraft(
                id: "attachment-1",
                name: "one.txt",
                mimeType: "text/plain",
                kind: .file,
                origin: .pastedText,
                data: Data("one".utf8)
            ),
        ], in: root).first)
        let unrelated = root.appendingPathComponent("unrelated.payload")
        try Data("keep".utf8).write(to: unrelated)

        NativeLocalAgentAttachmentStager().discard([
            reference,
            LocalAgentAttachmentReference(
                attachmentID: "unsafe",
                mediaType: "text/plain",
                payloadReference: "attachment-grant:../unrelated",
                payloadDigest: "sha256:" + String(repeating: "a", count: 64),
                byteSize: 1
            ),
        ], in: root)

        #expect(try FileManager.default.contentsOfDirectory(atPath: root.path) == [
            "unrelated.payload",
        ])
    }

    private func temporaryRoot() -> URL {
        URL(
            fileURLWithPath: "/tmp/chatos-attachment-\(UUID().uuidString.lowercased())",
            isDirectory: true
        )
    }
}
