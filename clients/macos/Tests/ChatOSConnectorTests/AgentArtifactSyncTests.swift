import ChatOSConnector
import ChatOSCore
import Foundation
import SQLite3
import XCTest

final class AgentArtifactSyncTests: XCTestCase {
    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-artifact-sync-\(UUID().uuidString)")
            .appendingPathComponent("group-chat.db")
    }

    func testMarkdownOutboxRetriesSyncsAndRestoresMissingLocalFile() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let remote = AgentArtifactRemoteStub(failuresBeforeSuccess: 1)
        let service = NativeAgentGroupChatService(
            databaseURL: url,
            agentArtifactService: remote
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(name: "项目经理", rolePrompt: "负责交付", modelConfigID: "model-1")
        )
        let room = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        let markdown = Data("# 方案\n\n完整内容".utf8)
        let posted = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .agent,
                senderID: agent.id,
                content: "方案已附上。",
                attachments: [
                    .init(
                        name: "方案.md",
                        mimeType: "text/markdown; charset=utf-8",
                        kind: .file,
                        origin: .file,
                        data: markdown
                    ),
                ]
            )
        )
        let attachmentID = try XCTUnwrap(posted.message.attachmentItems.first?.id)
        XCTAssertEqual(posted.message.attachmentItems.first?.syncStatus, .queued)
        XCTAssertNotNil(posted.message.attachmentItems.first?.sha256)

        let firstSyncCount = try await service.syncPendingAgentArtifacts(ownerUserID: "alice")
        XCTAssertEqual(firstSyncCount, 0)
        var messages = try await store.listMessages(ownerUserID: "alice", roomID: room.id)
        var attachment = try XCTUnwrap(messages.last?.attachmentItems.first)
        XCTAssertEqual(attachment.syncStatus, .failed)
        XCTAssertEqual(attachment.uploadError, "云端同步暂时失败，请稍后重试。")

        let resumedService = NativeAgentGroupChatService(
            databaseURL: url,
            agentArtifactService: remote
        )
        let resumedStore = try await resumedService.store()
        try await resumedStore.retryAgentArtifactUpload(
            ownerUserID: "alice",
            attachmentID: attachmentID
        )
        let secondSyncCount = try await resumedService.syncPendingAgentArtifacts(
            ownerUserID: "alice"
        )
        XCTAssertEqual(secondSyncCount, 1)
        let idempotentSyncCount = try await resumedService.syncPendingAgentArtifacts(
            ownerUserID: "alice"
        )
        XCTAssertEqual(idempotentSyncCount, 0)
        let uploadCount = await remote.uploadCount()
        XCTAssertEqual(uploadCount, 1)
        messages = try await resumedStore.listMessages(ownerUserID: "alice", roomID: room.id)
        attachment = try XCTUnwrap(messages.last?.attachmentItems.first)
        XCTAssertEqual(attachment.syncStatus, .synced)
        XCTAssertNotNil(attachment.artifactID)
        XCTAssertNil(attachment.uploadError)
        let remotePage = try await resumedService.remoteAgentArtifacts()
        XCTAssertEqual(
            remotePage.artifacts.map(\.artifactID),
            [try XCTUnwrap(attachment.artifactID)]
        )
        XCTAssertEqual(remotePage.artifacts.first?.name, "方案.md")

        let localResult = try await resumedStore.messageAttachment(
            ownerUserID: "alice",
            roomID: room.id,
            messageID: posted.message.id,
            attachmentID: attachmentID
        )
        let localPayload = try XCTUnwrap(localResult)
        try FileManager.default.removeItem(at: localPayload.localFileURL)
        let restoredResult = try await resumedStore.messageAttachment(
            ownerUserID: "alice",
            roomID: room.id,
            messageID: posted.message.id,
            attachmentID: attachmentID
        )
        let restored = try XCTUnwrap(restoredResult)
        XCTAssertEqual(try Data(contentsOf: restored.localFileURL), markdown)
        let downloadCount = await remote.downloadCount()
        XCTAssertEqual(downloadCount, 1)
    }

    func testLegacyAttachmentTableMigratesWithoutLosingRows() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        var database: OpaquePointer?
        XCTAssertEqual(sqlite3_open(url.path, &database), SQLITE_OK)
        guard let database else { return XCTFail("database open failed") }
        let legacySQL = """
        CREATE TABLE project_agent_message_attachments (
            owner_user_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            id TEXT NOT NULL,
            position INTEGER NOT NULL,
            name TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            kind TEXT NOT NULL,
            origin TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            PRIMARY KEY(owner_user_id, id)
        );
        INSERT INTO project_agent_message_attachments VALUES (
            'alice', 'message-1', 'attachment-1', 0, 'legacy.txt', 'text/plain',
            6, 'file', 'file', 'message-1/attachment-1'
        );
        """
        XCTAssertEqual(sqlite3_exec(database, legacySQL, nil, nil, nil), SQLITE_OK)
        sqlite3_close(database)

        _ = try SQLiteAgentGroupChatStore(databaseURL: url)

        XCTAssertTrue(try tableColumns(url, table: "project_agent_message_attachments")
            .isSuperset(of: [
                "sha256", "sync_status", "artifact_id", "storage_provider", "bucket",
                "object_key", "remote_view_path", "upload_error", "synced_at_unix_ms",
                "upload_attempt", "next_retry_at_unix_ms",
            ]))
        XCTAssertEqual(
            try scalarText(
                url,
                sql: "SELECT sync_status FROM project_agent_message_attachments WHERE id='attachment-1'"
            ),
            "local_only"
        )
    }

    private func tableColumns(_ url: URL, table: String) throws -> Set<String> {
        var database: OpaquePointer?
        guard sqlite3_open_v2(url.path, &database, SQLITE_OPEN_READONLY, nil) == SQLITE_OK,
              let database else { throw AgentGroupChatError.storage("open failed") }
        defer { sqlite3_close(database) }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, "PRAGMA table_info(\(table))", -1, &statement, nil)
                == SQLITE_OK,
              let statement else { throw AgentGroupChatError.storage("prepare failed") }
        defer { sqlite3_finalize(statement) }
        var columns = Set<String>()
        while sqlite3_step(statement) == SQLITE_ROW, let text = sqlite3_column_text(statement, 1) {
            columns.insert(String(cString: text))
        }
        return columns
    }

    private func scalarText(_ url: URL, sql: String) throws -> String? {
        var database: OpaquePointer?
        guard sqlite3_open_v2(url.path, &database, SQLITE_OPEN_READONLY, nil) == SQLITE_OK,
              let database else { throw AgentGroupChatError.storage("open failed") }
        defer { sqlite3_close(database) }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else { throw AgentGroupChatError.storage("prepare failed") }
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW,
              let value = sqlite3_column_text(statement, 0) else { return nil }
        return String(cString: value)
    }
}

private enum AgentArtifactRemoteStubError: Error { case offline }

private actor AgentArtifactRemoteStub: AgentArtifactRemoteServing {
    private var failuresBeforeSuccess: Int
    private var artifacts: [String: Data] = [:]
    private var metadataByID: [String: AgentArtifactRemoteItem] = [:]
    private var downloads = 0
    private var successfulUploads = 0

    init(failuresBeforeSuccess: Int) {
        self.failuresBeforeSuccess = failuresBeforeSuccess
    }

    func upload(_ request: AgentArtifactUploadRequest) async throws -> AgentArtifactRemoteMetadata {
        if failuresBeforeSuccess > 0 {
            failuresBeforeSuccess -= 1
            throw AgentArtifactRemoteStubError.offline
        }
        let artifactID = "artifact_0123456789abcdef0123456789abcdef"
        artifacts[artifactID] = request.data
        metadataByID[artifactID] = .init(
            artifactID: artifactID,
            name: request.name,
            mimeType: request.mimeType,
            size: request.data.count,
            sha256: request.sha256,
            status: "uploaded",
            remoteViewPath: "/api/agent-artifacts/\(artifactID)/content",
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
        successfulUploads += 1
        return .init(
            artifactID: artifactID,
            name: request.name,
            mimeType: request.mimeType,
            size: request.data.count,
            sha256: request.sha256,
            storageProvider: "minio",
            bucket: "private-bucket",
            objectKey: "private-object-key",
            remoteViewPath: "/api/agent-artifacts/\(artifactID)/content"
        )
    }

    func list(limit: Int, cursor: String?) async throws -> AgentArtifactRemotePage {
        .init(
            artifacts: Array(metadataByID.values.sorted {
                $0.artifactID < $1.artifactID
            }.prefix(limit)),
            nextCursor: nil
        )
    }

    func download(artifactID: String) async throws -> Data {
        downloads += 1
        guard let data = artifacts[artifactID] else { throw AgentArtifactRemoteStubError.offline }
        return data
    }

    func delete(artifactID: String) async throws {
        artifacts.removeValue(forKey: artifactID)
        metadataByID.removeValue(forKey: artifactID)
    }

    func downloadCount() -> Int { downloads }
    func uploadCount() -> Int { successfulUploads }
}
