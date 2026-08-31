import ChatOSCore
import Foundation
import SQLite3

enum ClipboardHistoryStoreError: LocalizedError {
    case databaseUnavailable
    case database(String)
    case payloadMissing

    var errorDescription: String? {
        switch self {
        case .databaseUnavailable: "Clipboard history database is unavailable."
        case let .database(message): message
        case .payloadMissing: "The clipboard payload is no longer available."
        }
    }
}

actor ClipboardHistoryStore {
    private let rootURL: URL
    private let payloadDirectoryURL: URL
    nonisolated(unsafe) private var database: OpaquePointer?

    init(rootURL: URL = ClipboardHistoryStore.defaultRootURL) {
        self.rootURL = rootURL
        self.payloadDirectoryURL = rootURL.appendingPathComponent("Payloads", isDirectory: true)
        do {
            try FileManager.default.createDirectory(
                at: payloadDirectoryURL,
                withIntermediateDirectories: true
            )
            var handle: OpaquePointer?
            let databaseURL = rootURL.appendingPathComponent("clipboard.sqlite")
            guard sqlite3_open_v2(
                databaseURL.path,
                &handle,
                SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX,
                nil
            ) == SQLITE_OK else {
                sqlite3_close(handle)
                return
            }
            database = handle
            try Self.execute(database: handle, sql: "PRAGMA journal_mode=WAL;")
            try Self.execute(database: handle, sql: "PRAGMA synchronous=NORMAL;")
            try Self.execute(database: handle, sql: """
                CREATE TABLE IF NOT EXISTS clipboard_entries (
                    id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    created_at REAL NOT NULL,
                    updated_at REAL NOT NULL,
                    content_hash TEXT NOT NULL UNIQUE,
                    text_preview TEXT,
                    source_bundle_id TEXT,
                    payload_reference TEXT NOT NULL,
                    byte_count INTEGER NOT NULL,
                    is_pinned INTEGER NOT NULL DEFAULT 0
                );
                """)
            try Self.execute(
                database: handle,
                sql: "CREATE INDEX IF NOT EXISTS clipboard_entries_updated ON clipboard_entries(is_pinned DESC, updated_at DESC);"
            )
        } catch {
            sqlite3_close(database)
            database = nil
        }
    }

    deinit {
        sqlite3_close(database)
    }

    func add(
        payload: ClipboardHistoryPayload,
        contentHash: String,
        preview: String?,
        sourceBundleID: String?
    ) throws -> ClipboardHistoryEntry {
        guard database != nil else { throw ClipboardHistoryStoreError.databaseUnavailable }
        let now = Date()
        if let existing = try entry(contentHash: contentHash) {
            let statement = try prepare("""
                UPDATE clipboard_entries
                SET updated_at = ?, source_bundle_id = COALESCE(?, source_bundle_id)
                WHERE content_hash = ?;
                """)
            defer { sqlite3_finalize(statement) }
            sqlite3_bind_double(statement, 1, now.timeIntervalSince1970)
            bind(sourceBundleID, to: 2, statement: statement)
            bind(contentHash, to: 3, statement: statement)
            try stepDone(statement)
            return ClipboardHistoryEntry(
                id: existing.id,
                kind: existing.kind,
                createdAt: existing.createdAt,
                updatedAt: now,
                contentHash: existing.contentHash,
                textPreview: existing.textPreview,
                sourceApplicationBundleID: sourceBundleID ?? existing.sourceApplicationBundleID,
                payloadReference: existing.payloadReference,
                byteCount: existing.byteCount,
                isPinned: existing.isPinned
            )
        }

        let id = UUID()
        let encoded = try Self.encode(payload: payload)
        let payloadReference = "Payloads/\(id.uuidString).\(encoded.extensionName)"
        let payloadURL = rootURL.appendingPathComponent(payloadReference)
        try encoded.data.write(to: payloadURL, options: .atomic)
        if case let .image(_, pasteboardType) = payload {
            try pasteboardType.write(
                to: payloadURL.deletingPathExtension().appendingPathExtension("type"),
                atomically: true,
                encoding: .utf8
            )
        }

        let statement = try prepare("""
            INSERT INTO clipboard_entries
            (id, kind, created_at, updated_at, content_hash, text_preview, source_bundle_id, payload_reference, byte_count, is_pinned)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0);
            """)
        defer { sqlite3_finalize(statement) }
        bind(id.uuidString, to: 1, statement: statement)
        bind(payload.kind.rawValue, to: 2, statement: statement)
        sqlite3_bind_double(statement, 3, now.timeIntervalSince1970)
        sqlite3_bind_double(statement, 4, now.timeIntervalSince1970)
        bind(contentHash, to: 5, statement: statement)
        bind(preview, to: 6, statement: statement)
        bind(sourceBundleID, to: 7, statement: statement)
        bind(payloadReference, to: 8, statement: statement)
        sqlite3_bind_int64(statement, 9, sqlite3_int64(encoded.data.count))
        do {
            try stepDone(statement)
        } catch {
            try? FileManager.default.removeItem(at: payloadURL)
            throw error
        }
        try cleanupIfNeeded()
        return ClipboardHistoryEntry(
            id: id,
            kind: payload.kind,
            createdAt: now,
            updatedAt: now,
            contentHash: contentHash,
            textPreview: preview,
            sourceApplicationBundleID: sourceBundleID,
            payloadReference: payloadReference,
            byteCount: Int64(encoded.data.count),
            isPinned: false
        )
    }

    func entries(limit: Int = 500) throws -> [ClipboardHistoryEntry] {
        let statement = try prepare("""
            SELECT id, kind, created_at, updated_at, content_hash, text_preview,
                   source_bundle_id, payload_reference, byte_count, is_pinned
            FROM clipboard_entries
            ORDER BY is_pinned DESC, updated_at DESC
            LIMIT ?;
            """)
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_int(statement, 1, Int32(min(Int(Int32.max), max(1, limit))))
        var result: [ClipboardHistoryEntry] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            if let entry = Self.decodeEntry(statement) {
                result.append(entry)
            }
        }
        return result
    }

    func payload(for entry: ClipboardHistoryEntry) throws -> ClipboardHistoryPayload {
        let url = rootURL.appendingPathComponent(entry.payloadReference)
        guard let data = try? Data(contentsOf: url) else {
            throw ClipboardHistoryStoreError.payloadMissing
        }
        switch entry.kind {
        case .text:
            return .text(String(decoding: data, as: UTF8.self))
        case .url:
            guard let url = URL(string: String(decoding: data, as: UTF8.self)) else {
                throw ClipboardHistoryStoreError.payloadMissing
            }
            return .url(url)
        case .files:
            return .files(try JSONDecoder().decode([URL].self, from: data))
        case .image:
            let typeURL = url.deletingPathExtension().appendingPathExtension("type")
            let type = (try? String(contentsOf: typeURL, encoding: .utf8)) ?? "public.png"
            return .image(data: data, pasteboardType: type)
        }
    }

    func setPinned(_ pinned: Bool, id: UUID) throws {
        let statement = try prepare("UPDATE clipboard_entries SET is_pinned = ? WHERE id = ?;")
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_int(statement, 1, pinned ? 1 : 0)
        bind(id.uuidString, to: 2, statement: statement)
        try stepDone(statement)
    }

    func delete(id: UUID) throws {
        guard let entry = try entry(id: id) else { return }
        let statement = try prepare("DELETE FROM clipboard_entries WHERE id = ?;")
        defer { sqlite3_finalize(statement) }
        bind(id.uuidString, to: 1, statement: statement)
        try stepDone(statement)
        removePayload(for: entry)
    }

    func clear() throws {
        let existing = try entries(limit: Int.max)
        try execute("DELETE FROM clipboard_entries;")
        existing.forEach(removePayload)
    }

    private func cleanupIfNeeded() throws {
        let all = try entries(limit: 2_000)
        let cutoff = Date().addingTimeInterval(-30 * 86_400)
        var unpinnedSeen = 0
        for entry in all where !entry.isPinned {
            unpinnedSeen += 1
            if unpinnedSeen > 500 || entry.updatedAt < cutoff {
                try delete(id: entry.id)
            }
        }
    }

    private func entry(contentHash: String) throws -> ClipboardHistoryEntry? {
        try singleEntry(sql: """
            SELECT id, kind, created_at, updated_at, content_hash, text_preview,
                   source_bundle_id, payload_reference, byte_count, is_pinned
            FROM clipboard_entries WHERE content_hash = ? LIMIT 1;
            """, value: contentHash)
    }

    private func entry(id: UUID) throws -> ClipboardHistoryEntry? {
        try singleEntry(sql: """
            SELECT id, kind, created_at, updated_at, content_hash, text_preview,
                   source_bundle_id, payload_reference, byte_count, is_pinned
            FROM clipboard_entries WHERE id = ? LIMIT 1;
            """, value: id.uuidString)
    }

    private func singleEntry(sql: String, value: String) throws -> ClipboardHistoryEntry? {
        let statement = try prepare(sql)
        defer { sqlite3_finalize(statement) }
        bind(value, to: 1, statement: statement)
        guard sqlite3_step(statement) == SQLITE_ROW else { return nil }
        return Self.decodeEntry(statement)
    }

    private func removePayload(for entry: ClipboardHistoryEntry) {
        let url = rootURL.appendingPathComponent(entry.payloadReference)
        try? FileManager.default.removeItem(at: url)
        try? FileManager.default.removeItem(
            at: url.deletingPathExtension().appendingPathExtension("type")
        )
    }

    private func execute(_ sql: String) throws {
        guard let database else { throw ClipboardHistoryStoreError.databaseUnavailable }
        try Self.execute(database: database, sql: sql)
    }

    private static func execute(database: OpaquePointer?, sql: String) throws {
        guard let database else { throw ClipboardHistoryStoreError.databaseUnavailable }
        var message: UnsafeMutablePointer<CChar>?
        guard sqlite3_exec(database, sql, nil, nil, &message) == SQLITE_OK else {
            let value = message.map { String(cString: $0) } ?? "SQLite operation failed."
            sqlite3_free(message)
            throw ClipboardHistoryStoreError.database(value)
        }
    }

    private func prepare(_ sql: String) throws -> OpaquePointer {
        guard let database else { throw ClipboardHistoryStoreError.databaseUnavailable }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw ClipboardHistoryStoreError.database(String(cString: sqlite3_errmsg(database)))
        }
        return statement
    }

    private func stepDone(_ statement: OpaquePointer?) throws {
        guard sqlite3_step(statement) == SQLITE_DONE else {
            let message = database.map { String(cString: sqlite3_errmsg($0)) }
                ?? "SQLite operation failed."
            throw ClipboardHistoryStoreError.database(message)
        }
    }

    private func bind(_ value: String?, to index: Int32, statement: OpaquePointer?) {
        guard let value else {
            sqlite3_bind_null(statement, index)
            return
        }
        sqlite3_bind_text(statement, index, value, -1, Self.sqliteTransient)
    }

    private static func decodeEntry(_ statement: OpaquePointer?) -> ClipboardHistoryEntry? {
        guard let idText = text(statement, column: 0),
              let id = UUID(uuidString: idText),
              let kindText = text(statement, column: 1),
              let kind = ClipboardContentKind(rawValue: kindText),
              let contentHash = text(statement, column: 4),
              let payloadReference = text(statement, column: 7) else {
            return nil
        }
        return ClipboardHistoryEntry(
            id: id,
            kind: kind,
            createdAt: Date(timeIntervalSince1970: sqlite3_column_double(statement, 2)),
            updatedAt: Date(timeIntervalSince1970: sqlite3_column_double(statement, 3)),
            contentHash: contentHash,
            textPreview: text(statement, column: 5),
            sourceApplicationBundleID: text(statement, column: 6),
            payloadReference: payloadReference,
            byteCount: Int64(sqlite3_column_int64(statement, 8)),
            isPinned: sqlite3_column_int(statement, 9) != 0
        )
    }

    private static func text(_ statement: OpaquePointer?, column: Int32) -> String? {
        guard sqlite3_column_type(statement, column) != SQLITE_NULL,
              let value = sqlite3_column_text(statement, column) else { return nil }
        return String(cString: value)
    }

    private static func encode(
        payload: ClipboardHistoryPayload
    ) throws -> (data: Data, extensionName: String) {
        switch payload {
        case let .text(value):
            return (Data(value.utf8), "txt")
        case let .url(value):
            return (Data(value.absoluteString.utf8), "url")
        case let .files(values):
            return (try JSONEncoder().encode(values), "files")
        case let .image(data, pasteboardType):
            let extensionName = pasteboardType.contains("tiff") ? "tiff" : "png"
            return (data, extensionName)
        }
    }

    private static let sqliteTransient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

    static let defaultRootURL: URL = FileManager.default.urls(
        for: .applicationSupportDirectory,
        in: .userDomainMask
    )[0]
        .appendingPathComponent("ChatOS", isDirectory: true)
        .appendingPathComponent("ClipboardHistory", isDirectory: true)
}
