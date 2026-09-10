import ChatOSCore
import Foundation
import SQLite3

/// Account-scoped, local-only authority. Opening a corrupt database fails closed; it never
/// becomes an empty registry. No API client, connector pairing or plugin installation is needed.
public actor SQLiteProjectRegistry: ProjectRegistry {
    private nonisolated(unsafe) var database: OpaquePointer?

    public init(databaseURL: URL) throws {
        try FileManager.default.createDirectory(
            at: databaseURL.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        var handle: OpaquePointer?
        guard sqlite3_open_v2(databaseURL.path, &handle, SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX, nil) == SQLITE_OK else {
            let message = handle.map { String(cString: sqlite3_errmsg($0)) } ?? "open failed"
            sqlite3_close(handle)
            throw ProjectRegistryError.storage(message)
        }
        do {
            sqlite3_busy_timeout(handle, 5_000)
            guard sqlite3_exec(handle, Self.schema, nil, nil, nil) == SQLITE_OK else {
                throw ProjectRegistryError.storage(String(cString: sqlite3_errmsg(handle)))
            }
            database = handle
        } catch {
            sqlite3_close(handle)
            throw error
        }
    }

    deinit { sqlite3_close(database) }

    public func list(ownerUserID: String, includeInactive: Bool = false) throws -> [LocalProjectRecord] {
        try ProjectRegistryValidation.identifier(ownerUserID, field: "ownerUserID")
        return try query(
            "SELECT \(Self.columns) FROM local_project_records WHERE owner_user_id = ?"
                + (includeInactive ? "" : " AND status = 'active'") + " ORDER BY name, id",
            [.text(ownerUserID)], row: readRecord
        )
    }

    public func get(ownerUserID: String, id: String) throws -> LocalProjectRecord? {
        try ProjectRegistryValidation.identifier(ownerUserID, field: "ownerUserID")
        try ProjectRegistryValidation.identifier(id, field: "id")
        return try query(
            "SELECT \(Self.columns) FROM local_project_records WHERE owner_user_id = ? AND id = ?",
            [.text(ownerUserID), .text(id)], row: readRecord
        ).first
    }

    public func create(ownerUserID: String, draft: LocalProjectDraft) throws -> LocalProjectRecord {
        let now = Self.now()
        let record = LocalProjectRecord(
            id: UUID().uuidString.lowercased(), ownerUserID: ownerUserID, draft: draft,
            createdAtUnixMs: now, updatedAtUnixMs: now
        )
        try record.validate()
        try insert(record)
        return record
    }

    public func update(
        ownerUserID: String, id: String, expectedRevision: Int64,
        draft: LocalProjectDraft, status: LocalProjectStatus
    ) throws -> LocalProjectRecord {
        try transaction {
            guard let old = try get(ownerUserID: ownerUserID, id: id) else { throw ProjectRegistryError.notFound }
            guard old.revision == expectedRevision else { throw ProjectRegistryError.revisionConflict }
            guard old.status != .removed else { throw ProjectRegistryError.removed }
            let record = LocalProjectRecord(
                id: id, ownerUserID: ownerUserID, draft: draft, revision: old.revision + 1,
                status: status, createdAtUnixMs: old.createdAtUnixMs,
                updatedAtUnixMs: max(Self.now(), old.updatedAtUnixMs)
            )
            try record.validate()
            try execute("""
                UPDATE local_project_records SET name = ?, description = ?, workspace_id = ?,
                relative_root = ?, revision = ?, status = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND revision = ?
                """, [
                    .text(draft.name), .text(draft.description), .text(draft.workspaceID), .text(draft.relativeRoot),
                    .integer(record.revision), .text(status.rawValue), .integer(record.updatedAtUnixMs),
                    .text(ownerUserID), .text(id), .integer(expectedRevision),
                ])
            guard sqlite3_changes(database) == 1 else { throw ProjectRegistryError.revisionConflict }
            return record
        }
    }

    private func insert(_ record: LocalProjectRecord) throws {
        try execute("INSERT INTO local_project_records (\(Self.columns)) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", [
            .text(record.ownerUserID), .text(record.id), .text(record.draft.name), .text(record.draft.description),
            .text(record.draft.workspaceID), .text(record.draft.relativeRoot), .integer(record.revision),
            .text(record.status.rawValue), .integer(record.createdAtUnixMs), .integer(record.updatedAtUnixMs),
        ])
    }

    private func readRecord(_ statement: OpaquePointer) throws -> LocalProjectRecord {
        guard let status = LocalProjectStatus(rawValue: Self.string(statement, 7)) else {
            throw ProjectRegistryError.storage("invalid project status")
        }
        let record = LocalProjectRecord(
            id: Self.string(statement, 1), ownerUserID: Self.string(statement, 0),
            draft: LocalProjectDraft(name: Self.string(statement, 2), description: Self.string(statement, 3),
                                     workspaceID: Self.string(statement, 4), relativeRoot: Self.string(statement, 5)),
            revision: sqlite3_column_int64(statement, 6), status: status,
            createdAtUnixMs: sqlite3_column_int64(statement, 8), updatedAtUnixMs: sqlite3_column_int64(statement, 9)
        )
        try record.validate()
        return record
    }

    private enum Value { case text(String), integer(Int64) }

    private func query<T>(_ sql: String, _ values: [Value] = [], row: (OpaquePointer) throws -> T) throws -> [T] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK, let statement else { throw storageError() }
        defer { sqlite3_finalize(statement) }
        for (offset, value) in values.enumerated() {
            let index = Int32(offset + 1)
            let result: Int32
            switch value {
            case let .text(text):
                result = sqlite3_bind_text(statement, index, text, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self))
            case let .integer(number): result = sqlite3_bind_int64(statement, index, number)
            }
            guard result == SQLITE_OK else { throw storageError() }
        }
        var rows: [T] = []
        while true {
            switch sqlite3_step(statement) {
            case SQLITE_ROW: rows.append(try row(statement))
            case SQLITE_DONE: return rows
            default: throw storageError()
            }
        }
    }

    private func execute(_ sql: String, _ values: [Value] = []) throws {
        let _: [Int] = try query(sql, values) { _ in 0 }
    }

    private func transaction<T>(_ body: () throws -> T) throws -> T {
        try execute("BEGIN IMMEDIATE")
        do {
            let result = try body()
            try execute("COMMIT")
            return result
        } catch {
            try? execute("ROLLBACK")
            throw error
        }
    }

    private func storageError() -> ProjectRegistryError {
        .storage(String(cString: sqlite3_errmsg(database)))
    }

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        String(cString: sqlite3_column_text(statement, index))
    }

    private static func now() -> Int64 { Int64(Date().timeIntervalSince1970 * 1_000) }

    private static let columns = "owner_user_id, id, name, description, workspace_id, relative_root, revision, status, created_at_unix_ms, updated_at_unix_ms"
    private static let schema = """
        PRAGMA journal_mode = WAL;
        BEGIN IMMEDIATE;
        CREATE TABLE IF NOT EXISTS local_project_records (
            owner_user_id TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL,
            description TEXT NOT NULL, workspace_id TEXT NOT NULL, relative_root TEXT NOT NULL,
            revision INTEGER NOT NULL CHECK(revision > 0),
            status TEXT NOT NULL CHECK(status IN ('active', 'archived', 'removed')),
            created_at_unix_ms INTEGER NOT NULL, updated_at_unix_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_user_id, id)
        );
        CREATE TABLE IF NOT EXISTS local_project_schema_migrations (version INTEGER PRIMARY KEY NOT NULL);
        INSERT OR IGNORE INTO local_project_schema_migrations(version) VALUES (1);
        COMMIT;
        """
}
