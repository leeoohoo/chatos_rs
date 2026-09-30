@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentArtifactClientTests: XCTestCase {
    func testCRUDCommandsRemainOwnerScopedAndCarryContent() async throws {
        let host = ArtifactHostStub()
        let client = NativeLocalAgentArtifactClient(host: host)
        let data = Data("# Local\n".utf8)
        let metadata = try await client.store(
            ownerUserID: "owner-1",
            request: .init(
                name: "local.md",
                mimeType: "text/markdown",
                data: data,
                sha256: String(repeating: "a", count: 64),
                idempotencyKey: "attachment-1"
            )
        )
        XCTAssertEqual(metadata.artifactID, "artifact-1")
        var command = try await host.lastCommand()
        XCTAssertEqual(command.type, "create_artifact")
        XCTAssertEqual(command.ownerUserID, "owner-1")
        XCTAssertEqual(command.dataBase64, data.base64EncodedString())

        let page = try await client.list(ownerUserID: "owner-1", limit: 50, cursor: nil)
        XCTAssertEqual(page.artifacts.map(\.artifactID), ["artifact-1"])
        let downloaded = try await client.read(
            ownerUserID: "owner-1",
            artifactID: "artifact-1"
        )
        XCTAssertEqual(downloaded, data)
        try await client.remove(ownerUserID: "owner-1", artifactID: "artifact-1")
        command = try await host.lastCommand()
        XCTAssertEqual(command.type, "delete_artifact")
        XCTAssertEqual(command.ownerUserID, "owner-1")
    }
}

private actor ArtifactHostStub: LocalAgentHostClientServicing {
    private let data = Data("# Local\n".utf8)
    private var commands: [Data] = []

    func start(ownerUserID _: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: command) as? [String: Any])
        switch object["type"] as? String {
        case "create_artifact":
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
            dataBase64: object["data_base64"] as? String
        )
    }

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
    let dataBase64: String?
}
