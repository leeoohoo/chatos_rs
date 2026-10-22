import CryptoKit
import Foundation
import Testing
@testable import ChatOSConnector

struct AgentArtifactUploadDataLoaderTests {
    @Test
    func verifiedArtifactLoadsAcrossFixedSizeChunks() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        let loaded = try await AgentArtifactUploadDataLoader.load(
            from: fixture.source,
            expectedBytes: fixture.data.count,
            expectedSHA256: fixture.sha256
        )

        #expect(loaded == fixture.data)
    }

    @Test
    func digestMismatchIsRejected() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        await #expect(throws: AgentArtifactUploadDataError.self) {
            _ = try await AgentArtifactUploadDataLoader.load(
                from: fixture.source,
                expectedBytes: fixture.data.count,
                expectedSHA256: String(repeating: "0", count: 64)
            )
        }
    }

    @Test
    func symbolicLinkSourceIsRejected() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let link = fixture.root.appendingPathComponent("linked-artifact")
        try FileManager.default.createSymbolicLink(
            at: link,
            withDestinationURL: fixture.source
        )

        await #expect(throws: AgentArtifactUploadDataError.self) {
            _ = try await AgentArtifactUploadDataLoader.load(
                from: link,
                expectedBytes: fixture.data.count,
                expectedSHA256: fixture.sha256
            )
        }
    }

    private func makeFixture() throws -> (
        root: URL,
        source: URL,
        data: Data,
        sha256: String
    ) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "agent-artifact-upload-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let source = root.appendingPathComponent("source.artifact")
        let data = Data(repeating: 0x61, count: 600 * 1_024)
        try data.write(to: source)
        return (
            root,
            source,
            data,
            SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        )
    }
}
