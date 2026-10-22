@testable import ChatOSConnector
import ChatOSCore
import CryptoKit
import Foundation
import XCTest

final class NativeLocalAgentArtifactClientTests: XCTestCase {
    func testCRUDCommandsRemainOwnerScopedAndCarryContent() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "artifact-client-\(UUID().uuidString)",
            isDirectory: true
        )
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let artifacts = root.appendingPathComponent("artifacts", isDirectory: true)
        let staging = root.appendingPathComponent("staging", isDirectory: true)
        let host = ArtifactHostStub(stagingDirectoryURL: staging)
        let client = NativeLocalAgentArtifactClient(
            host: host,
            artifactDirectoryURL: artifacts,
            stagingDirectoryURL: staging
        )
        let data = Data("# Local\n".utf8)
        let source = root.appendingPathComponent("source.md")
        try data.write(to: source)
        let sha256 = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        let metadata = try await client.store(
            ownerUserID: "owner-1",
            request: .init(
                name: "local.md",
                mimeType: "text/markdown",
                fileURL: source,
                size: data.count,
                sha256: sha256,
                idempotencyKey: "attachment-1"
            )
        )
        XCTAssertEqual(metadata.artifactID, "artifact-1")
        var command = try await host.lastCommand()
        XCTAssertEqual(command.type, "create_artifact")
        XCTAssertEqual(command.ownerUserID, "owner-1")
        XCTAssertEqual(command.size, data.count)
        XCTAssertNotNil(command.stagingToken)
        XCTAssertNil(command.dataBase64)
        let uploaded = await host.uploadedData()
        XCTAssertEqual(uploaded, data)
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: staging.path),
            []
        )

        let page = try await client.list(ownerUserID: "owner-1", limit: 50, cursor: nil)
        XCTAssertEqual(page.artifacts.map(\.artifactID), ["artifact-1"])
        let downloaded = try await client.read(
            ownerUserID: "owner-1",
            artifactID: "artifact-1"
        )
        XCTAssertEqual(downloaded, data)
        let reference = try await client.fileReference(
            ownerUserID: "owner-1",
            artifactID: "artifact-1"
        )
        XCTAssertEqual(reference.fileURL, artifacts.appendingPathComponent("artifact-1.artifact"))
        XCTAssertEqual(reference.size, data.count)
        command = try await host.lastCommand()
        XCTAssertEqual(command.type, "get_artifact_file")
        XCTAssertEqual(command.ownerUserID, "owner-1")
        try await client.remove(ownerUserID: "owner-1", artifactID: "artifact-1")
        command = try await host.lastCommand()
        XCTAssertEqual(command.type, "delete_artifact")
        XCTAssertEqual(command.ownerUserID, "owner-1")
    }

    func testFileReferenceRejectsHostPathEscape() async throws {
        let host = ArtifactHostStub(relativePath: "../artifact-1.artifact")
        let client = NativeLocalAgentArtifactClient(
            host: host,
            artifactDirectoryURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("artifacts")
        )
        do {
            _ = try await client.fileReference(
                ownerUserID: "owner-1",
                artifactID: "artifact-1"
            )
            XCTFail("Escaping artifact path was accepted")
        } catch NativeLocalAgentHostError.invalidResponse {}
    }
}

private actor ArtifactHostStub: LocalAgentHostClientServicing {
    private let data = Data("# Local\n".utf8)
    private let relativePath: String
    private let stagingDirectoryURL: URL?
    private var commands: [Data] = []
    private var uploaded: Data?

    init(
        relativePath: String = "artifact-1.artifact",
        stagingDirectoryURL: URL? = nil
    ) {
        self.relativePath = relativePath
        self.stagingDirectoryURL = stagingDirectoryURL
    }

    func start(ownerUserID _: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: command) as? [String: Any])
        switch object["type"] as? String {
        case "create_artifact":
            let token = try XCTUnwrap(object["staging_token"] as? String)
            let stagingDirectoryURL = try XCTUnwrap(stagingDirectoryURL)
            uploaded = try Data(
                contentsOf: stagingDirectoryURL.appendingPathComponent("\(token).staged")
            )
            return try json(["type": "artifact", "artifact": artifact])
        case "list_artifacts":
            return try json([
                "type": "artifacts",
                "page": ["artifacts": [artifact], "next_cursor": NSNull()],
            ])
        case "get_artifact_data":
            return try json([
                "type": "artifact_data",
                "artifact_id": "artifact-1",
                "data_base64": data.base64EncodedString(),
            ])
        case "get_artifact_file":
            return try json([
                "type": "artifact_file",
                "file": [
                    "artifact_id": "artifact-1",
                    "relative_path": relativePath,
                    "size": data.count,
                    "sha256": String(repeating: "a", count: 64),
                ],
            ])
        case "delete_artifact":
            return try json(["type": "artifact_deleted", "artifact_id": "artifact-1"])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    func lastCommand() throws -> ArtifactCommandSnapshot {
        let command = try XCTUnwrap(commands.last)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: command) as? [String: Any])
        return .init(
            type: object["type"] as? String,
            ownerUserID: object["owner_user_id"] as? String,
            stagingToken: object["staging_token"] as? String,
            size: object["size"] as? Int,
            dataBase64: object["data_base64"] as? String
        )
    }

    func uploadedData() -> Data? { uploaded }

    private var artifact: [String: Any] {
        [
            "artifact_id": "artifact-1",
            "owner_user_id": "owner-1",
            "name": "local.md",
            "mime_type": "text/markdown",
            "size": data.count,
            "sha256": String(repeating: "a", count: 64),
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 1,
        ]
    }

    private func json(_ object: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: object)
    }
}

private struct ArtifactCommandSnapshot: Sendable {
    let type: String?
    let ownerUserID: String?
    let stagingToken: String?
    let size: Int?
    let dataBase64: String?
}
