import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class SQLiteProjectRegistryTests: XCTestCase {
    private let draft = LocalProjectDraft(name: "项目", workspaceID: "workspace-1", relativeRoot: "apps/example")

    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("project-registry-\(UUID().uuidString)/projects.db")
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

}
