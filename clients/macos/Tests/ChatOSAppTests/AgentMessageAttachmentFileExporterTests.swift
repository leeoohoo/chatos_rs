import ChatOSCore
import CryptoKit
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class AgentMessageAttachmentFileExporterTests: XCTestCase {
    func testVerifiedAttachmentAtomicallyReplacesDestinationAndKeepsSource() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        try Data("old destination".utf8).write(to: fixture.destination)

        try await AgentMessageAttachmentFileExporter.export(
            payload(for: fixture, sha256: fixture.sha256),
            to: fixture.destination
        )

        XCTAssertEqual(try Data(contentsOf: fixture.destination), fixture.data)
        XCTAssertEqual(try Data(contentsOf: fixture.source), fixture.data)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: fixture.root.path)
            .allSatisfy { !$0.hasPrefix(".chatos-agent-export-") })
    }

    func testDigestMismatchPreservesExistingDestination() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let existing = Data("keep me".utf8)
        try existing.write(to: fixture.destination)

        do {
            try await AgentMessageAttachmentFileExporter.export(
                payload(for: fixture, sha256: String(repeating: "0", count: 64)),
                to: fixture.destination
            )
            XCTFail("Mismatched digest was exported")
        } catch AgentAttachmentPresentationError.invalidSize {
            XCTAssertEqual(try Data(contentsOf: fixture.destination), existing)
        }
    }

    func testSymbolicLinkSourceIsRejectedWithoutChangingDestination() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let link = fixture.root.appendingPathComponent("linked.bin")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: fixture.source)
        let existing = Data("keep me".utf8)
        try existing.write(to: fixture.destination)

        do {
            try await AgentMessageAttachmentFileExporter.export(
                payload(for: fixture, source: link, sha256: fixture.sha256),
                to: fixture.destination
            )
            XCTFail("Symbolic link was exported")
        } catch AgentAttachmentPresentationError.invalidSize {
            XCTAssertEqual(try Data(contentsOf: fixture.destination), existing)
        }
    }

    func testCancellationPreservesDestinationAndRemovesTemporaryFile() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let existing = Data("keep me".utf8)
        try existing.write(to: fixture.destination)
        let task = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            try await AgentMessageAttachmentFileExporter.export(
                payload(for: fixture, sha256: fixture.sha256),
                to: fixture.destination
            )
        }

        do {
            try await task.value
            XCTFail("Cancelled export completed")
        } catch is CancellationError {}

        XCTAssertEqual(try Data(contentsOf: fixture.destination), existing)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: fixture.root.path)
            .allSatisfy { !$0.hasPrefix(".chatos-agent-export-") })
    }

    private func payload(
        for fixture: (root: URL, source: URL, destination: URL, data: Data, sha256: String),
        source: URL? = nil,
        sha256: String
    ) -> ProjectAgentMessageAttachmentPayload {
        ProjectAgentMessageAttachmentPayload(
            attachment: .init(
                id: "attachment-1",
                name: "attachment.bin",
                mimeType: "application/octet-stream",
                size: fixture.data.count,
                kind: .file,
                origin: .file,
                sha256: sha256
            ),
            localFileURL: source ?? fixture.source
        )
    }

    private func makeFixture() throws -> (
        root: URL,
        source: URL,
        destination: URL,
        data: Data,
        sha256: String
    ) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "AgentMessageAttachmentExportTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let data = Data((0..<(700 * 1_024)).lazy.map { UInt8($0 % 251) })
        let source = root.appendingPathComponent("source.bin")
        try data.write(to: source)
        let destination = root.appendingPathComponent("destination.bin")
        let sha256 = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        return (root, source, destination, data, sha256)
    }
}
