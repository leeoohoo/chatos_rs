@testable import ChatOSConnector
import ChatOSCore
import ChatOSCodeReadWorkerClient
import CryptoKit
import Foundation
import XCTest

final class NativeLocalAgentArtifactClientTests: XCTestCase {
    func testFailedAttachmentRetriesAndRestoresWithRelocatedLibraryUsingRealProcesses() async throws {
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let hostExecutable = ProcessInfo.processInfo.environment["CHATOS_LOCAL_AGENT_HOST_PATH"]
            .map { URL(fileURLWithPath: $0) }
            ?? repository.appendingPathComponent("target-shared/debug/chatos_local_agent_host")
        let workerExecutable = repository.appendingPathComponent(
            "clients/macos/.build/out/Products/Debug/ChatOSCodeReadWorker"
        )
        guard FileManager.default.isExecutableFile(atPath: hostExecutable.path),
              FileManager.default.isExecutableFile(atPath: workerExecutable.path) else {
            if ProcessInfo.processInfo.environment["CHATOS_REQUIRE_NATIVE_HOST_TESTS"] == "1" {
                XCTFail("Required native Host or code read worker integration executable is missing")
                throw NativeLocalAgentHostError.notRunning
            }
            throw XCTSkip("Build the native Host and code read worker for artifact integration tests")
        }
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("artifact-relocation-\(UUID().uuidString)")
        let volumeRoot = ProcessInfo.processInfo.environment["CHATOS_ARTIFACT_TEST_VOLUME"]
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
            ?? FileManager.default.temporaryDirectory
        let relocated = volumeRoot.appendingPathComponent("artifact-library-\(UUID().uuidString)")
        defer {
            try? FileManager.default.removeItem(at: root)
            try? FileManager.default.removeItem(at: relocated)
        }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: relocated, withIntermediateDirectories: true)
        if ProcessInfo.processInfo.environment["CHATOS_ARTIFACT_TEST_VOLUME"] != nil {
            let sourceAttributes = try FileManager.default.attributesOfItem(atPath: root.path)
            let destinationAttributes = try FileManager.default.attributesOfItem(atPath: relocated.path)
            XCTAssertNotEqual(sourceAttributes[.systemNumber] as? NSNumber,
                              destinationAttributes[.systemNumber] as? NSNumber)
        }
        let alias = root.appendingPathComponent("LocalAgent")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: relocated)
        let host = NativeLocalAgentHostLifecycle(configuration: .init(
            executableURL: hostExecutable,
            databaseURL: alias.appendingPathComponent("local-agent.sqlite3")
        ))
        do {
            try await host.startForBootstrap(ownerUserID: "alice")
            let worker = CodeReadWorkerProcessClient(executableURL: workerExecutable)
            let client = NativeLocalAgentArtifactClient(
                host: host,
                artifactDirectoryURL: alias.appendingPathComponent("local-agent.artifacts"),
                stagingDirectoryURL: alias.appendingPathComponent("local-agent.artifact-staging"),
                codeReadWorker: worker
            )
            let service = NativeAgentGroupChatService(
                databaseURL: root.appendingPathComponent("AgentGroupChat.sqlite3"),
                agentArtifactStore: client, codeReadWorker: worker
            )
            let store = try await service.store()
            let agent = try await store.createAgent(
                ownerUserID: "alice",
                draft: .init(name: "文档助手", rolePrompt: "交付文档", modelConfigID: "test-model")
            )
            let room = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)
            let data = Data(("# 方向决策简报\n" + String(repeating: "中文内容\n", count: 24_000)).utf8)
            let posted = try await store.postMessage(
                ownerUserID: "alice", roomID: room.id,
                draft: .init(senderKind: .agent, senderID: agent.id, content: "文档已附上。",
                             attachments: [.init(name: "简报.md", mimeType: "text/markdown; charset=utf-8",
                                                 kind: .file, origin: .file, data: data)])
            )
            let jobResult = try await store.claimNextAgentArtifactStorage(ownerUserID: "alice", nowUnixMs: 1)
            let job = try XCTUnwrap(jobResult)
            try await store.markAgentArtifactStorageFailed(
                ownerUserID: "alice", attachmentID: job.attachmentID,
                attempt: job.attempt, error: "旧版本归档失败", nowUnixMs: 1
            )
            try await service.retryAgentArtifactStorage(
                ownerUserID: "alice", roomID: room.id, attachmentID: job.attachmentID
            )
            let messages = try await store.listMessages(ownerUserID: "alice", roomID: room.id)
            let attachment = try XCTUnwrap(messages.last?.attachmentItems.first)
            XCTAssertEqual(attachment.syncStatus, .synced)
            XCTAssertNil(attachment.uploadError)
            let artifactID = try XCTUnwrap(attachment.artifactID)
            let archived = try await client.read(ownerUserID: "alice", artifactID: artifactID)
            XCTAssertEqual(archived, data)
            try FileManager.default.removeItem(at: job.request.fileURL)
            let payload = try await store.messageAttachment(
                ownerUserID: "alice", roomID: room.id,
                messageID: posted.message.id, attachmentID: attachment.id
            )
            XCTAssertEqual(try Data(contentsOf: XCTUnwrap(payload).localFileURL), data)
            await host.stop()
        } catch {
            await host.stop()
            throw error
        }
    }

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
            stagingDirectoryURL: staging,
            codeReadWorker: ConnectorCoreCodeReadWorker()
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
