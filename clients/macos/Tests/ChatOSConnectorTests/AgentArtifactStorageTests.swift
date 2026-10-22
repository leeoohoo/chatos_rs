import ChatOSConnector
import ChatOSCore
import Foundation
import SQLite3
import XCTest

final class AgentArtifactStorageTests: XCTestCase {
    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-artifact-sync-\(UUID().uuidString)")
            .appendingPathComponent("group-chat.db")
    }

    func testMarkdownArtifactRetriesStoresAndRestoresMissingLocalFile() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let artifactStore = AgentArtifactStoreStub(failuresBeforeSuccess: 1)
        let service = NativeAgentGroupChatService(
            databaseURL: url,
            agentArtifactStore: artifactStore
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

        let firstStorageCount = try await service.persistPendingAgentArtifacts(ownerUserID: "alice")
        XCTAssertEqual(firstStorageCount, 0)
        var messages = try await store.listMessages(ownerUserID: "alice", roomID: room.id)
        var attachment = try XCTUnwrap(messages.last?.attachmentItems.first)
        XCTAssertEqual(attachment.syncStatus, .failed)
        XCTAssertEqual(attachment.uploadError, "本地文档存储暂时失败，请稍后重试。")

        let resumedService = NativeAgentGroupChatService(
            databaseURL: url,
            agentArtifactStore: artifactStore
        )
        let resumedStore = try await resumedService.store()
        try await resumedStore.retryAgentArtifactStorage(
            ownerUserID: "alice",
            attachmentID: attachmentID
        )
        let secondStorageCount = try await resumedService.persistPendingAgentArtifacts(
            ownerUserID: "alice"
        )
        XCTAssertEqual(secondStorageCount, 1)
        let idempotentStorageCount = try await resumedService.persistPendingAgentArtifacts(
            ownerUserID: "alice"
        )
        XCTAssertEqual(idempotentStorageCount, 0)
        let storeCount = await artifactStore.storeCount()
        XCTAssertEqual(storeCount, 1)
        messages = try await resumedStore.listMessages(ownerUserID: "alice", roomID: room.id)
        attachment = try XCTUnwrap(messages.last?.attachmentItems.first)
        XCTAssertEqual(attachment.syncStatus, .synced)
        XCTAssertNotNil(attachment.artifactID)
        XCTAssertNil(attachment.uploadError)
        let localPage = try await resumedService.localAgentArtifacts(ownerUserID: "alice")
        XCTAssertEqual(
            localPage.artifacts.map(\.artifactID),
            [try XCTUnwrap(attachment.artifactID)]
        )
        XCTAssertEqual(localPage.artifacts.first?.name, "方案.md")

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
        let downloadCount = await artifactStore.downloadCount()
        XCTAssertEqual(downloadCount, 1)
    }

    func testMissingSyncedAttachmentRestoresFromFileWithoutDataDownload() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let artifactStore = FileAgentArtifactStoreStub(
            rootURL: url.deletingLastPathComponent().appendingPathComponent("artifacts")
        )
        let fixture = try await makeSyncedAttachment(
            databaseURL: url,
            artifactStore: artifactStore
        )
        let localResult = try await fixture.store.messageAttachment(
            ownerUserID: "alice",
            roomID: fixture.roomID,
            messageID: fixture.messageID,
            attachmentID: fixture.attachmentID
        )
        let localPayload = try XCTUnwrap(localResult)
        try FileManager.default.removeItem(at: localPayload.localFileURL)

        let restoredResult = try await fixture.store.messageAttachment(
            ownerUserID: "alice",
            roomID: fixture.roomID,
            messageID: fixture.messageID,
            attachmentID: fixture.attachmentID
        )
        let restored = try XCTUnwrap(restoredResult)

        XCTAssertEqual(try Data(contentsOf: restored.localFileURL), fixture.data)
        let sourceURL = await artifactStore.sourceURL()
        let fileReferenceCount = await artifactStore.fileReferenceCount()
        let downloadCount = await artifactStore.downloadCount()
        XCTAssertTrue(FileManager.default.fileExists(atPath: sourceURL.path))
        XCTAssertEqual(fileReferenceCount, 1)
        XCTAssertEqual(downloadCount, 0)
    }

    func testFileRestoreRejectsDigestMismatchWithoutInstallingAttachment() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let artifactStore = FileAgentArtifactStoreStub(
            rootURL: url.deletingLastPathComponent().appendingPathComponent("artifacts")
        )
        let fixture = try await makeSyncedAttachment(
            databaseURL: url,
            artifactStore: artifactStore
        )
        let localResult = try await fixture.store.messageAttachment(
            ownerUserID: "alice",
            roomID: fixture.roomID,
            messageID: fixture.messageID,
            attachmentID: fixture.attachmentID
        )
        let localPayload = try XCTUnwrap(localResult)
        try FileManager.default.removeItem(at: localPayload.localFileURL)
        try await artifactStore.replaceSource(
            with: Data(repeating: 0x78, count: fixture.data.count)
        )

        do {
            _ = try await fixture.store.messageAttachment(
                ownerUserID: "alice",
                roomID: fixture.roomID,
                messageID: fixture.messageID,
                attachmentID: fixture.attachmentID
            )
            XCTFail("expected integrity failure")
        } catch let error as AgentGroupChatError {
            XCTAssertEqual(
                error,
                .storage("restored message attachment failed integrity check")
            )
        }

        XCTAssertFalse(FileManager.default.fileExists(atPath: localPayload.localFileURL.path))
        XCTAssertEqual(try restoreTemporaryFiles(beside: localPayload.localFileURL), [])
        let downloadCount = await artifactStore.downloadCount()
        XCTAssertEqual(downloadCount, 0)
    }

    func testFileRestoreRejectsSymbolicLinkSource() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let artifactStore = FileAgentArtifactStoreStub(
            rootURL: url.deletingLastPathComponent().appendingPathComponent("artifacts")
        )
        let fixture = try await makeSyncedAttachment(
            databaseURL: url,
            artifactStore: artifactStore
        )
        let localResult = try await fixture.store.messageAttachment(
            ownerUserID: "alice",
            roomID: fixture.roomID,
            messageID: fixture.messageID,
            attachmentID: fixture.attachmentID
        )
        let localPayload = try XCTUnwrap(localResult)
        try FileManager.default.removeItem(at: localPayload.localFileURL)
        try await artifactStore.replaceSourceWithSymbolicLink()

        do {
            _ = try await fixture.store.messageAttachment(
                ownerUserID: "alice",
                roomID: fixture.roomID,
                messageID: fixture.messageID,
                attachmentID: fixture.attachmentID
            )
            XCTFail("expected symlink rejection")
        } catch let error as AgentGroupChatError {
            XCTAssertEqual(
                error,
                .storage("restored message attachment failed integrity check")
            )
        }

        XCTAssertFalse(FileManager.default.fileExists(atPath: localPayload.localFileURL.path))
        XCTAssertEqual(try restoreTemporaryFiles(beside: localPayload.localFileURL), [])
        let downloadCount = await artifactStore.downloadCount()
        XCTAssertEqual(downloadCount, 0)
    }

    func testFileRestoreRejectsReferenceMetadataMismatchWithoutDataFallback() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let artifactStore = FileAgentArtifactStoreStub(
            rootURL: url.deletingLastPathComponent().appendingPathComponent("artifacts")
        )
        let fixture = try await makeSyncedAttachment(
            databaseURL: url,
            artifactStore: artifactStore
        )
        let localResult = try await fixture.store.messageAttachment(
            ownerUserID: "alice",
            roomID: fixture.roomID,
            messageID: fixture.messageID,
            attachmentID: fixture.attachmentID
        )
        let localPayload = try XCTUnwrap(localResult)
        try FileManager.default.removeItem(at: localPayload.localFileURL)
        await artifactStore.setReferenceMetadataMismatch()

        do {
            _ = try await fixture.store.messageAttachment(
                ownerUserID: "alice",
                roomID: fixture.roomID,
                messageID: fixture.messageID,
                attachmentID: fixture.attachmentID
            )
            XCTFail("expected metadata rejection")
        } catch let error as AgentGroupChatError {
            XCTAssertEqual(
                error,
                .storage("restored message attachment failed integrity check")
            )
        }

        XCTAssertFalse(FileManager.default.fileExists(atPath: localPayload.localFileURL.path))
        let fileReferenceCount = await artifactStore.fileReferenceCount()
        let downloadCount = await artifactStore.downloadCount()
        XCTAssertEqual(fileReferenceCount, 1)
        XCTAssertEqual(downloadCount, 0)
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

    private func makeSyncedAttachment(
        databaseURL: URL,
        artifactStore: any AgentArtifactServing
    ) async throws -> (
        store: SQLiteAgentGroupChatStore,
        roomID: String,
        messageID: String,
        attachmentID: String,
        data: Data
    ) {
        let service = NativeAgentGroupChatService(
            databaseURL: databaseURL,
            agentArtifactStore: artifactStore
        )
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(name: "文件恢复助手", rolePrompt: "恢复附件", modelConfigID: "model-1")
        )
        let room = try await store.openHumanAgentDirect(
            ownerUserID: "alice",
            agentID: agent.id
        )
        let data = Data("# 文件恢复\n\n流式校验内容".utf8)
        let posted = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .agent,
                senderID: agent.id,
                content: "附件",
                attachments: [
                    .init(
                        name: "恢复.md",
                        mimeType: "text/markdown; charset=utf-8",
                        kind: .file,
                        origin: .file,
                        data: data
                    ),
                ]
            )
        )
        let persistedCount = try await service.persistPendingAgentArtifacts(ownerUserID: "alice")
        XCTAssertEqual(persistedCount, 1)
        return (
            store,
            room.id,
            posted.message.id,
            try XCTUnwrap(posted.message.attachmentItems.first?.id),
            data
        )
    }

    private func restoreTemporaryFiles(beside fileURL: URL) throws -> [String] {
        try FileManager.default.contentsOfDirectory(
            atPath: fileURL.deletingLastPathComponent().path
        ).filter { $0.hasPrefix(".chatos-artifact-restore-") }
    }
}

private enum AgentArtifactStoreStubError: Error { case offline }

private actor AgentArtifactStoreStub: AgentArtifactServing {
    private var failuresBeforeSuccess: Int
    private var artifacts: [String: Data] = [:]
    private var metadataByID: [String: AgentArtifactItem] = [:]
    private var downloads = 0
    private var successfulStores = 0

    init(failuresBeforeSuccess: Int) {
        self.failuresBeforeSuccess = failuresBeforeSuccess
    }

    func store(
        ownerUserID: String,
        request: AgentArtifactWriteRequest
    ) async throws -> AgentArtifactMetadata {
        XCTAssertEqual(ownerUserID, "alice")
        if failuresBeforeSuccess > 0 {
            failuresBeforeSuccess -= 1
            throw AgentArtifactStoreStubError.offline
        }
        let artifactID = "artifact_0123456789abcdef0123456789abcdef"
        artifacts[artifactID] = request.data
        metadataByID[artifactID] = .init(
            artifactID: artifactID,
            name: request.name,
            mimeType: request.mimeType,
            size: request.data.count,
            sha256: request.sha256,
            status: "stored",
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
        successfulStores += 1
        return .init(
            artifactID: artifactID,
            name: request.name,
            mimeType: request.mimeType,
            size: request.data.count,
            sha256: request.sha256
        )
    }

    func list(ownerUserID: String, limit: Int, cursor: String?) async throws -> AgentArtifactPage {
        XCTAssertEqual(ownerUserID, "alice")
        return .init(
            artifacts: Array(metadataByID.values.sorted {
                $0.artifactID < $1.artifactID
            }.prefix(limit)),
            nextCursor: nil
        )
    }

    func read(ownerUserID: String, artifactID: String) async throws -> Data {
        XCTAssertEqual(ownerUserID, "alice")
        downloads += 1
        guard let data = artifacts[artifactID] else { throw AgentArtifactStoreStubError.offline }
        return data
    }

    func remove(ownerUserID: String, artifactID: String) async throws {
        XCTAssertEqual(ownerUserID, "alice")
        artifacts.removeValue(forKey: artifactID)
        metadataByID.removeValue(forKey: artifactID)
    }

    func downloadCount() -> Int { downloads }
    func storeCount() -> Int { successfulStores }
}

private actor FileAgentArtifactStoreStub: AgentArtifactFileServing {
    private let rootURL: URL
    private let artifactID = "artifact_0123456789abcdef0123456789abcdef"
    private var item: AgentArtifactItem?
    private var reads = 0
    private var fileReferences = 0
    private var metadataMismatch = false

    init(rootURL: URL) {
        self.rootURL = rootURL
    }

    func store(
        ownerUserID: String,
        request: AgentArtifactWriteRequest
    ) async throws -> AgentArtifactMetadata {
        XCTAssertEqual(ownerUserID, "alice")
        try FileManager.default.createDirectory(
            at: rootURL,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try request.data.write(to: sourceURL(), options: .atomic)
        item = .init(
            artifactID: artifactID,
            name: request.name,
            mimeType: request.mimeType,
            size: request.data.count,
            sha256: request.sha256,
            status: "stored",
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
        return .init(
            artifactID: artifactID,
            name: request.name,
            mimeType: request.mimeType,
            size: request.data.count,
            sha256: request.sha256
        )
    }

    func list(ownerUserID: String, limit: Int, cursor: String?) async throws -> AgentArtifactPage {
        XCTAssertEqual(ownerUserID, "alice")
        return .init(artifacts: item.map { [$0] } ?? [], nextCursor: nil)
    }

    func read(ownerUserID: String, artifactID: String) async throws -> Data {
        XCTAssertEqual(ownerUserID, "alice")
        XCTAssertEqual(artifactID, self.artifactID)
        reads += 1
        return try Data(contentsOf: sourceURL())
    }

    func fileReference(
        ownerUserID: String,
        artifactID: String
    ) async throws -> AgentArtifactFileReference {
        XCTAssertEqual(ownerUserID, "alice")
        XCTAssertEqual(artifactID, self.artifactID)
        fileReferences += 1
        let item = try XCTUnwrap(item)
        return .init(
            fileURL: sourceURL(),
            size: item.size + (metadataMismatch ? 1 : 0),
            sha256: item.sha256
        )
    }

    func remove(ownerUserID: String, artifactID: String) async throws {
        XCTAssertEqual(ownerUserID, "alice")
        XCTAssertEqual(artifactID, self.artifactID)
        try? FileManager.default.removeItem(at: sourceURL())
        item = nil
    }

    func replaceSource(with data: Data) throws {
        try data.write(to: sourceURL(), options: .atomic)
    }

    func replaceSourceWithSymbolicLink() throws {
        let source = sourceURL()
        let target = rootURL.appendingPathComponent("symlink-target")
        let data = try Data(contentsOf: source)
        try data.write(to: target)
        try FileManager.default.removeItem(at: source)
        try FileManager.default.createSymbolicLink(at: source, withDestinationURL: target)
    }

    func setReferenceMetadataMismatch() {
        metadataMismatch = true
    }

    func sourceURL() -> URL {
        rootURL.appendingPathComponent("\(artifactID).artifact")
    }

    func downloadCount() -> Int { reads }
    func fileReferenceCount() -> Int { fileReferences }
}
