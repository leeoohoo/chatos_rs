import CryptoKit
import Foundation
import Testing
@testable import ChatOSConnector

struct AgentArtifactUploadStagerTests {
    @Test
    func verifiedArtifactStagesAcrossFixedSizeChunks() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        let staged = try await AgentArtifactUploadStager.stage(
            from: fixture.source,
            in: fixture.staging,
            expectedBytes: fixture.data.count,
            expectedSHA256: fixture.sha256,
            codeReadWorker: ConnectorCoreCodeReadWorker()
        )
        defer { AgentArtifactUploadStager.discard(staged) }

        #expect(staged.fileURL.lastPathComponent == "\(staged.token).staged")
        #expect(try Data(contentsOf: staged.fileURL) == fixture.data)
        let attributes = try FileManager.default.attributesOfItem(atPath: staged.fileURL.path)
        #expect((attributes[.posixPermissions] as? NSNumber)?.intValue == 0o600)
    }

    @Test
    func digestMismatchLeavesNoStagingFile() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        await #expect(throws: AgentArtifactUploadStagingError.self) {
            _ = try await AgentArtifactUploadStager.stage(
                from: fixture.source,
                in: fixture.staging,
                expectedBytes: fixture.data.count,
                expectedSHA256: String(repeating: "0", count: 64),
                codeReadWorker: ConnectorCoreCodeReadWorker()
            )
        }
        let children = try FileManager.default.contentsOfDirectory(
            at: fixture.staging,
            includingPropertiesForKeys: nil
        )
        #expect(children.isEmpty)
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

        await #expect(throws: AgentArtifactUploadStagingError.self) {
            _ = try await AgentArtifactUploadStager.stage(
                from: link,
                in: fixture.staging,
                expectedBytes: fixture.data.count,
                expectedSHA256: fixture.sha256,
                codeReadWorker: ConnectorCoreCodeReadWorker()
            )
        }
    }

    private func makeFixture() throws -> (
        root: URL,
        source: URL,
        staging: URL,
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
            root.appendingPathComponent("staging", isDirectory: true),
            data,
            SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        )
    }
}
