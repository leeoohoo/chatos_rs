import CryptoKit
import ChatOSCodeReadWorkerProtocol
import ChatOSCodeReadWorkerClient
import Foundation
import Testing
@testable import ChatOSConnector

struct AgentArtifactUploadStagerTests {
    @Test
    func relocatedArtifactDirectoryStagesAndRestoresThroughConfiguredAlias() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let library = fixture.root.appendingPathComponent("relocated-library")
        try FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)
        let alias = fixture.root.appendingPathComponent("LocalAgent")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: library)
        let staging = alias.appendingPathComponent("nested/artifact-staging")
        let worker = ConnectorCoreCodeReadWorker()

        // The previous shared-root operation rejects the configured LocalAgent alias.
        await #expect(throws: CodeReadWorkerProcessClientError.self) {
            _ = try await worker.execute(
                operation: "artifact_stage_v1",
                arguments: [
                    "source_path": .string("source.artifact"),
                    "destination_path": .string("LocalAgent/nested/artifact-staging/00000000-0000-0000-0000-000000000001.staged"),
                    "token": .string("00000000-0000-0000-0000-000000000001"),
                    "byte_size": .number(Double(fixture.data.count)),
                    "sha256": .string(fixture.sha256),
                ],
                projectRoot: fixture.root.resolvingSymlinksInPath(),
                requestCWD: nil, defaultToolRoot: nil, ripgrepURL: nil
            )
        }
        let staged = try await AgentArtifactUploadStager.stage(
            from: fixture.source, in: staging,
            expectedBytes: fixture.data.count, expectedSHA256: fixture.sha256,
            codeReadWorker: worker
        )
        defer { AgentArtifactUploadStager.discard(staged) }
        #expect(try Data(contentsOf: staged.fileURL) == fixture.data)
        #expect(staged.fileURL.path.hasPrefix(library.resolvingSymlinksInPath().path + "/"))
        let restored = fixture.root.appendingPathComponent("restored/document.md")
        try await AgentArtifactFileRestorer.restore(
            .init(fileURL: staging.appendingPathComponent(staged.fileURL.lastPathComponent),
                  size: fixture.data.count, sha256: fixture.sha256),
            to: restored, expectedBytes: fixture.data.count, expectedSHA256: fixture.sha256,
            codeReadWorker: worker
        )
        #expect(try Data(contentsOf: restored) == fixture.data)
    }

    @Test
    func changedDestinationIdentityAndInsertedSymlinkAreRejected() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        try FileManager.default.createDirectory(at: fixture.staging, withIntermediateDirectories: true)
        let destination = fixture.staging.appendingPathComponent("nested/restored.md")
        let scope = try AgentArtifactWorkerPathScope(source: fixture.source, destination: destination)
        let moved = fixture.root.appendingPathComponent("old-staging")
        try FileManager.default.moveItem(at: fixture.staging, to: moved)
        try FileManager.default.createDirectory(at: fixture.staging, withIntermediateDirectories: true)
        let arguments = scope.destinationArguments.merging([
            "source_path": .string(scope.sourcePath),
            "destination_path": .string(scope.destinationPath),
            "byte_size": .number(Double(fixture.data.count)),
            "sha256": .string(fixture.sha256),
        ]) { _, value in value }
        let worker = ConnectorCoreCodeReadWorker()
        await #expect(throws: CodeReadWorkerProcessClientError.self) {
            _ = try await worker.execute(
                operation: "artifact_restore_v2", arguments: arguments,
                projectRoot: scope.root, requestCWD: nil, defaultToolRoot: nil, ripgrepURL: nil
            )
        }
        let freshScope = try AgentArtifactWorkerPathScope(source: fixture.source, destination: destination)
        try FileManager.default.createSymbolicLink(
            at: fixture.staging.appendingPathComponent("nested"), withDestinationURL: moved
        )
        await #expect(throws: CodeReadWorkerProcessClientError.self) {
            _ = try await worker.execute(
                operation: "artifact_restore_v2",
                arguments: arguments.merging(freshScope.destinationArguments) { _, value in value },
                projectRoot: freshScope.root, requestCWD: nil, defaultToolRoot: nil, ripgrepURL: nil
            )
        }
        #expect(!FileManager.default.fileExists(atPath: moved.appendingPathComponent("restored.md").path))
    }

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
