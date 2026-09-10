import ChatOSConnector
import ChatOSCore
import Foundation
import SQLite3
import XCTest

final class SQLiteProjectRegistryTests: XCTestCase {
    private let draft = LocalProjectDraft(name: "项目", workspaceID: "workspace-1", relativeRoot: "apps/example")

    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("project-registry-\(UUID().uuidString)/projects.db")
    }

    private func legacy(_ id: String = "legacy-id", owner: String = "alice") -> LocalProjectRecord {
        LocalProjectRecord(id: id, ownerUserID: owner, draft: draft, createdAtUnixMs: 1, updatedAtUnixMs: 2)
    }

    func testOfflineCreateSurvivesReopenAndIsAccountScoped() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let registry = try SQLiteProjectRegistry(databaseURL: url)
        let created = try await registry.create(ownerUserID: "alice", draft: draft)
        let reopened = try SQLiteProjectRegistry(databaseURL: url)
        let records = try await reopened.list(ownerUserID: "alice")
        let other = try await reopened.list(ownerUserID: "bob")
        let missing = try await reopened.get(ownerUserID: "bob", id: created.id)
        XCTAssertEqual(records, [created])
        XCTAssertTrue(other.isEmpty)
        XCTAssertNil(missing)
    }

    func testRenameRebindAndRePairKeepIdentityAndFrozenSnapshot() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let registry = try SQLiteProjectRegistry(databaseURL: url)
        let created = try await registry.create(ownerUserID: "alice", draft: draft)
        let snapshot = try ProjectContextSnapshot(record: created, deviceID: "old-device")
        let changed = try await registry.update(
            ownerUserID: "alice", id: created.id, expectedRevision: 1,
            draft: .init(name: "renamed", workspaceID: "workspace-2", relativeRoot: "moved"), status: .active
        )
        let newSnapshot = try ProjectContextSnapshot(record: changed, deviceID: "new-device")
        XCTAssertEqual(created.id, changed.id)
        XCTAssertEqual(changed.revision, 2)
        XCTAssertEqual(snapshot.executionTarget.relativeRoot, "apps/example")
        XCTAssertEqual(snapshot.executionTarget.deviceId, "old-device")
        XCTAssertEqual(newSnapshot.executionTarget.deviceId, "new-device")
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(snapshot)) as? [String: Any])
        XCTAssertEqual(Set(json.keys), ["schemaVersion", "projectId", "projectName", "projectRevision", "executionTarget"])
        XCTAssertEqual(json["projectId"] as? String, created.id)
    }

    func testConcurrentWritersCannotOverwriteRevision() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let registry = try SQLiteProjectRegistry(databaseURL: url)
        let second = try SQLiteProjectRegistry(databaseURL: url)
        let created = try await registry.create(ownerUserID: "alice", draft: draft)
        _ = try await registry.update(ownerUserID: "alice", id: created.id, expectedRevision: 1, draft: draft, status: .archived)
        do {
            _ = try await second.update(ownerUserID: "alice", id: created.id, expectedRevision: 1, draft: draft, status: .active)
            XCTFail("Stale write accepted")
        } catch { XCTAssertEqual(error as? ProjectRegistryError, .revisionConflict) }
        let active = try await registry.list(ownerUserID: "alice")
        XCTAssertTrue(active.isEmpty)
        let restored = try await registry.update(ownerUserID: "alice", id: created.id, expectedRevision: 2, draft: draft, status: .active)
        XCTAssertEqual(restored.revision, 3)
    }

    func testImportPreservesIDsIsIdempotentAndNeverResurrectsTombstone() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let registry = try SQLiteProjectRegistry(databaseURL: url)
        let record = legacy()
        let result = try await registry.importRecords(ownerUserID: "alice", sourceID: "export-1", records: [record])
        let replay = try await registry.importRecords(ownerUserID: "alice", sourceID: "export-1", records: [record])
        XCTAssertEqual(result.insertedIDs, [record.id])
        XCTAssertEqual(replay, result)
        _ = try await registry.update(ownerUserID: "alice", id: record.id, expectedRevision: 1, draft: draft, status: .removed)
        let later = try await registry.importRecords(ownerUserID: "alice", sourceID: "export-2", records: [record])
        XCTAssertEqual(later.skippedIDs, [record.id])
        let active = try await registry.list(ownerUserID: "alice")
        let removed = try await registry.get(ownerUserID: "alice", id: record.id)
        XCTAssertTrue(active.isEmpty)
        XCTAssertEqual(removed?.status, .removed)
        do {
            _ = try await registry.update(ownerUserID: "alice", id: record.id, expectedRevision: 2, draft: draft, status: .active)
            XCTFail("Tombstone restored")
        } catch { XCTAssertEqual(error as? ProjectRegistryError, .removed) }
    }

    func testImportRejectsWrongOwnerDuplicatesAndChangedReceiptWithoutPartialWrites() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let registry = try SQLiteProjectRegistry(databaseURL: url)
        for records in [[legacy(), legacy("bad", owner: "bob")], [legacy(), legacy()]] {
            do {
                _ = try await registry.importRecords(ownerUserID: "alice", sourceID: "invalid", records: records)
                XCTFail("Invalid import accepted")
            } catch { XCTAssertTrue(error is ProjectRegistryError) }
        }
        let empty = try await registry.list(ownerUserID: "alice", includeInactive: true)
        XCTAssertTrue(empty.isEmpty)
        _ = try await registry.importRecords(ownerUserID: "alice", sourceID: "export", records: [legacy()])
        do {
            _ = try await registry.importRecords(ownerUserID: "alice", sourceID: "export", records: [legacy("another")])
            XCTFail("Changed receipt accepted")
        } catch { XCTAssertEqual(error as? ProjectRegistryError, .importSourceConflict) }
        let missing = try await registry.get(ownerUserID: "alice", id: "another")
        XCTAssertNil(missing)
    }

    func testPortablePathValidationAndInactiveSnapshotRejection() throws {
        for path in ["/absolute", "../escape", "a/../b", "a/./b", "a//b", "a/", "C:/repo", "a\\b", "a\0b", " leading", "trailing "] {
            XCTAssertThrowsError(try LocalProjectDraft(name: "project", workspaceID: "ws", relativeRoot: path).validate(), path)
        }
        for path in ["", "子目录/repo", "space here", "a%20b"] {
            XCTAssertNoThrow(try LocalProjectDraft(name: "project", workspaceID: "ws", relativeRoot: path).validate())
        }
        let archived = LocalProjectRecord(id: "id", ownerUserID: "alice", draft: draft, status: .archived, createdAtUnixMs: 1, updatedAtUnixMs: 1)
        XCTAssertThrowsError(try ProjectContextSnapshot(record: archived, deviceID: "device"))
    }

    func testCorruptDatabaseDoesNotBecomeAnEmptyRegistry() throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("not a sqlite database".utf8).write(to: url)
        XCTAssertThrowsError(try SQLiteProjectRegistry(databaseURL: url))
    }

    func testImportRollsBackRecordsAndReceiptOnDatabaseFailure() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let registry = try SQLiteProjectRegistry(databaseURL: url)
        var database: OpaquePointer?
        XCTAssertEqual(sqlite3_open(url.path, &database), SQLITE_OK)
        defer { sqlite3_close(database) }
        XCTAssertEqual(sqlite3_exec(database, """
            CREATE TRIGGER fail_import BEFORE INSERT ON local_project_records
            WHEN NEW.id = 'b' BEGIN SELECT RAISE(ABORT, 'injected failure'); END;
            """, nil, nil, nil), SQLITE_OK)
        do {
            _ = try await registry.importRecords(ownerUserID: "alice", sourceID: "export", records: [legacy("a"), legacy("b")])
            XCTFail("Database failure ignored")
        } catch { XCTAssertTrue(error is ProjectRegistryError) }
        let records = try await registry.list(ownerUserID: "alice", includeInactive: true)
        XCTAssertTrue(records.isEmpty)
        let retry = try await registry.importRecords(ownerUserID: "alice", sourceID: "export", records: [legacy("a")])
        XCTAssertEqual(retry.insertedIDs, ["a"], "The receipt must roll back with the records")
    }
}
