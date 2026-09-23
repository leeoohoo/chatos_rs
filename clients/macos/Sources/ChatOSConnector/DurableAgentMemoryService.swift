import ChatOSAgentRuntime
import Foundation
import SQLite3

/// Adds a client-owned cache and durable upload queue in front of Memory Engine.
/// A local Agent can keep working while the service is unreachable; immutable
/// records are uploaded with their program-generated ids when connectivity returns.
public struct OfflineCapableAgentServiceProvider: AgentServiceProviding {
    private let upstream: any AgentServiceProviding
    private let store: SQLiteAgentMemoryCacheStore

    public init(upstream: any AgentServiceProviding, databaseURL: URL) throws {
        self.upstream = upstream
        self.store = try SQLiteAgentMemoryCacheStore(databaseURL: databaseURL)
    }

    public func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        try await upstream.makeAgentModel(configID: configID, policy: policy)
    }

    public func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy,
        thinkingLevel: String?
    ) async throws -> any AgentModelClient {
        try await upstream.makeAgentModel(
            configID: configID,
            policy: policy,
            thinkingLevel: thinkingLevel
        )
    }

    public func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        DurableAgentMemoryService(
            scope: scope,
            store: store,
            remoteFactory: { try await upstream.makeAgentMemory(scope: scope) }
        )
    }
}

actor DurableAgentMemoryService: AgentMemoryServicing {
    typealias RemoteFactory = @Sendable () async throws -> any AgentMemoryServicing

    private let scope: AgentMemoryScope
    private let store: SQLiteAgentMemoryCacheStore
    private let remoteFactory: RemoteFactory
    private var remote: (any AgentMemoryServicing)?

    init(
        scope: AgentMemoryScope,
        store: SQLiteAgentMemoryCacheStore,
        remoteFactory: @escaping RemoteFactory
    ) {
        self.scope = scope
        self.store = store
        self.remoteFactory = remoteFactory
    }

    func ensureThread() async throws {
        try await store.ensureScope(scope)
        guard await store.shouldAttemptRemote() else { return }
        do {
            let service = try await remoteService()
            try await service.ensureThread()
            try await flush(using: service)
            await store.recordRemoteSuccess()
        } catch {
            try Self.rethrowIntegrityFailure(error)
            remote = nil
            await store.recordRemoteFailure()
            // The local scope is already durable. Network recovery will retry
            // the same program-owned thread id and outbox records.
        }
    }

    func sync(_ entries: [AgentMemoryEntry], reconciling _: Bool) async throws {
        try await store.enqueue(entries, scope: scope)
        guard await store.shouldAttemptRemote() else { return }
        do {
            let service = try await remoteService()
            try await service.ensureThread()
            try await flush(using: service)
            await store.recordRemoteSuccess()
        } catch {
            try Self.rethrowIntegrityFailure(error)
            remote = nil
            await store.recordRemoteFailure()
        }
    }

    func compose() async throws -> AgentMemoryContext {
        guard await store.shouldAttemptRemote() else {
            return try await store.compose(scope: scope)
        }
        do {
            let service = try await remoteService()
            try await service.ensureThread()
            try await flush(using: service)
            let context = try await service.compose()
            try await store.save(context: context, scope: scope)
            await store.recordRemoteSuccess()
            return context
        } catch {
            try Self.rethrowIntegrityFailure(error)
            remote = nil
            await store.recordRemoteFailure()
            // Cached Memory plus records not represented in the last successful
            // compose is a complete local continuation boundary.
            return try await store.compose(scope: scope)
        }
    }

    private func remoteService() async throws -> any AgentMemoryServicing {
        if let remote { return remote }
        let created = try await remoteFactory()
        remote = created
        return created
    }

    private func flush(using service: any AgentMemoryServicing) async throws {
        while true {
            let batch = try await store.pending(scope: scope, limit: 32)
            guard !batch.isEmpty else { return }
            let wasAttempted = batch.contains(where: \.attempted)
            try await store.markAttempted(batch.map(\.entry.id), scope: scope)
            do {
                try await service.sync(batch.map(\.entry), reconciling: wasAttempted)
            } catch AgentContextError.syncUncertain {
                // A successful point lookup would have reconciled the batch.
                // `syncUncertain` means it is definitely absent, so the same
                // immutable ids can now be uploaded without rewriting records.
                try await service.sync(batch.map(\.entry), reconciling: false)
            }
            try await store.markUploaded(batch.map(\.entry.id), scope: scope)
        }
    }

    private static func rethrowIntegrityFailure(_ error: Error) throws {
        if let runtimeError = error as? AgentRuntimeError,
           case .scopeMismatch = runtimeError { throw error }
        if let contextError = error as? AgentContextError,
           case .invalidHistory = contextError { throw error }
    }
}

actor SQLiteAgentMemoryCacheStore {
    struct PendingEntry: Sendable {
        let entry: AgentMemoryEntry
        let attempted: Bool
    }

    private struct StoredEntry: Codable, Equatable {
        let id: String
        let index: Int
        let message: AgentMessage
        let createdAt: Date

        init(_ entry: AgentMemoryEntry) {
            id = entry.id
            index = entry.index
            message = entry.message
            createdAt = entry.createdAt
        }

        var entry: AgentMemoryEntry {
            .init(id: id, index: index, message: message, createdAt: createdAt)
        }

        func isEquivalent(to entry: AgentMemoryEntry) -> Bool {
            id == entry.id
                && index == entry.index
                && message == entry.message
                && abs(createdAt.timeIntervalSince(entry.createdAt)) <= 0.001
        }
    }

    private struct StoredContext: Codable {
        struct Block: Codable {
            let blockType: String
            let text: String
        }
        struct Record: Codable {
            let id: String
            let message: AgentMessage
        }

        let blocks: [Block]
        let records: [Record]

        init(_ context: AgentMemoryContext) {
            blocks = context.blocks.map { .init(blockType: $0.blockType, text: $0.text) }
            records = context.recentRecords.map { .init(id: $0.id, message: $0.message) }
        }

        var context: AgentMemoryContext {
            .init(
                blocks: blocks.map { .init(blockType: $0.blockType, text: $0.text) },
                recentRecords: records.map { .init(id: $0.id, message: $0.message) }
            )
        }
    }

    private nonisolated(unsafe) var database: OpaquePointer?
    private var remoteFailureCount = 0
    private var nextRemoteAttemptAt = Date.distantPast

    init(databaseURL: URL) throws {
        try FileManager.default.createDirectory(
            at: databaseURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        var handle: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &handle,
            SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK else {
            sqlite3_close(handle)
            throw AgentContextError.unavailable
        }
        sqlite3_busy_timeout(handle, 5_000)
        let schema = """
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS agent_memory_threads (
            thread_id TEXT PRIMARY KEY NOT NULL,
            tenant_id TEXT NOT NULL,
            source_id TEXT NOT NULL,
            subject_id TEXT NOT NULL,
            context_json BLOB,
            updated_at REAL NOT NULL
        );
        CREATE TABLE IF NOT EXISTS agent_memory_records (
            thread_id TEXT NOT NULL,
            record_id TEXT NOT NULL,
            message_index INTEGER NOT NULL,
            entry_json BLOB NOT NULL,
            pending INTEGER NOT NULL DEFAULT 1,
            attempted INTEGER NOT NULL DEFAULT 0,
            represented INTEGER NOT NULL DEFAULT 0,
            created_at REAL NOT NULL,
            PRIMARY KEY (thread_id, record_id),
            FOREIGN KEY (thread_id) REFERENCES agent_memory_threads(thread_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_agent_memory_outbox
            ON agent_memory_records(thread_id, pending, message_index);
        """
        guard sqlite3_exec(handle, schema, nil, nil, nil) == SQLITE_OK else {
            sqlite3_close(handle)
            throw AgentContextError.unavailable
        }
        database = handle
    }

    deinit { sqlite3_close(database) }

    func shouldAttemptRemote(now: Date = Date()) -> Bool {
        now >= nextRemoteAttemptAt
    }

    func recordRemoteSuccess() {
        remoteFailureCount = 0
        nextRemoteAttemptAt = .distantPast
    }

    func recordRemoteFailure(now: Date = Date()) {
        remoteFailureCount = min(remoteFailureCount + 1, 6)
        let delay = min(300.0, 15.0 * pow(2.0, Double(remoteFailureCount - 1)))
        nextRemoteAttemptAt = now.addingTimeInterval(delay)
    }

    func ensureScope(_ scope: AgentMemoryScope) throws {
        let existing = try rows(
            "SELECT tenant_id, source_id, subject_id FROM agent_memory_threads WHERE thread_id = ?",
            [.text(scope.threadID)]
        ) { statement in
            (text(statement, 0), text(statement, 1), text(statement, 2))
        }.first
        if let existing {
            guard existing == (scope.tenantID, scope.sourceID, scope.subjectID) else {
                throw AgentRuntimeError.scopeMismatch
            }
            return
        }
        try execute(
            "INSERT INTO agent_memory_threads (thread_id, tenant_id, source_id, subject_id, updated_at) VALUES (?, ?, ?, ?, ?)",
            [.text(scope.threadID), .text(scope.tenantID), .text(scope.sourceID),
             .text(scope.subjectID), .double(Date().timeIntervalSince1970)]
        )
    }

    func enqueue(_ entries: [AgentMemoryEntry], scope: AgentMemoryScope) throws {
        try ensureScope(scope)
        guard !entries.isEmpty, entries.count <= 32 else { throw AgentContextError.invalidHistory }
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .millisecondsSince1970
        encoder.outputFormatting = [.sortedKeys]
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .millisecondsSince1970
        try transaction {
            for entry in entries {
                guard entry.id == scope.recordID(at: entry.index) else {
                    throw AgentContextError.invalidHistory
                }
                let encoded = try encoder.encode(StoredEntry(entry))
                if let existing = try rows(
                    "SELECT entry_json FROM agent_memory_records WHERE thread_id = ? AND record_id = ?",
                    [.text(scope.threadID), .text(entry.id)],
                    row: { blob($0, 0) }
                ).first {
                    guard let stored = try? decoder.decode(StoredEntry.self, from: existing),
                          stored.isEquivalent(to: entry) else {
                        throw AgentContextError.invalidHistory
                    }
                    continue
                }
                try execute(
                    """
                    INSERT INTO agent_memory_records
                        (thread_id, record_id, message_index, entry_json, pending, attempted, represented, created_at)
                    VALUES (?, ?, ?, ?, 1, 0, 0, ?)
                    """,
                    [.text(scope.threadID), .text(entry.id), .integer(Int64(entry.index)),
                     .blob(encoded), .double(entry.createdAt.timeIntervalSince1970)]
                )
            }
        }
    }

    func pending(scope: AgentMemoryScope, limit: Int) throws -> [PendingEntry] {
        try ensureScope(scope)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .millisecondsSince1970
        return try rows(
            """
            SELECT entry_json, attempted FROM agent_memory_records
            WHERE thread_id = ? AND pending = 1
            ORDER BY created_at, message_index LIMIT ?
            """,
            [.text(scope.threadID), .integer(Int64(limit))]
        ) { statement in
            let stored = try decoder.decode(StoredEntry.self, from: blob(statement, 0))
            return PendingEntry(entry: stored.entry, attempted: sqlite3_column_int(statement, 1) != 0)
        }
    }

    func markAttempted(_ ids: [String], scope: AgentMemoryScope) throws {
        try updateRecords(ids, scope: scope, sql: "attempted = 1")
    }

    func markUploaded(_ ids: [String], scope: AgentMemoryScope) throws {
        try updateRecords(ids, scope: scope, sql: "pending = 0")
    }

    func save(context: AgentMemoryContext, scope: AgentMemoryScope) throws {
        try ensureScope(scope)
        let data = try JSONEncoder().encode(StoredContext(context))
        try transaction {
            try execute(
                "UPDATE agent_memory_threads SET context_json = ?, updated_at = ? WHERE thread_id = ?",
                [.blob(data), .double(Date().timeIntervalSince1970), .text(scope.threadID)]
            )
            try execute(
                "UPDATE agent_memory_records SET represented = 1 WHERE thread_id = ? AND pending = 0",
                [.text(scope.threadID)]
            )
        }
    }

    func compose(scope: AgentMemoryScope) throws -> AgentMemoryContext {
        try ensureScope(scope)
        let contextData = try rows(
            "SELECT context_json FROM agent_memory_threads WHERE thread_id = ?",
            [.text(scope.threadID)]
        ) { statement -> Data? in
            sqlite3_column_type(statement, 0) == SQLITE_NULL ? nil : blob(statement, 0)
        }.first ?? nil
        var context = try contextData.map { try JSONDecoder().decode(StoredContext.self, from: $0).context }
            ?? .init(blocks: [], recentRecords: [])
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .millisecondsSince1970
        let local = try rows(
            """
            SELECT entry_json FROM agent_memory_records
            WHERE thread_id = ? AND represented = 0
            ORDER BY created_at, message_index
            """,
            [.text(scope.threadID)]
        ) { statement in
            try decoder.decode(StoredEntry.self, from: blob(statement, 0))
        }
        var seen = Set(context.recentRecords.map(\.id))
        context = .init(
            blocks: context.blocks,
            recentRecords: context.recentRecords + local.compactMap { stored in
                guard seen.insert(stored.id).inserted else { return nil }
                return .init(id: stored.id, message: stored.message)
            }
        )
        return context
    }

    private func updateRecords(_ ids: [String], scope: AgentMemoryScope, sql: String) throws {
        guard !ids.isEmpty else { return }
        try transaction {
            for id in ids {
                try execute(
                    "UPDATE agent_memory_records SET \(sql) WHERE thread_id = ? AND record_id = ?",
                    [.text(scope.threadID), .text(id)]
                )
            }
        }
    }

    private enum Value {
        case text(String), integer(Int64), double(Double), blob(Data)
    }

    private func execute(_ sql: String, _ values: [Value]) throws {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK else {
            throw AgentContextError.unavailable
        }
        defer { sqlite3_finalize(statement) }
        try bind(values, to: statement)
        guard sqlite3_step(statement) == SQLITE_DONE else { throw AgentContextError.unavailable }
    }

    private func rows<T>(
        _ sql: String,
        _ values: [Value],
        row: (OpaquePointer) throws -> T
    ) throws -> [T] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else { throw AgentContextError.unavailable }
        defer { sqlite3_finalize(statement) }
        try bind(values, to: statement)
        var result: [T] = []
        while true {
            switch sqlite3_step(statement) {
            case SQLITE_ROW: result.append(try row(statement))
            case SQLITE_DONE: return result
            default: throw AgentContextError.unavailable
            }
        }
    }

    private func bind(_ values: [Value], to statement: OpaquePointer?) throws {
        let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        for (offset, value) in values.enumerated() {
            let index = Int32(offset + 1)
            let result: Int32
            switch value {
            case let .text(value): result = sqlite3_bind_text(statement, index, value, -1, transient)
            case let .integer(value): result = sqlite3_bind_int64(statement, index, value)
            case let .double(value): result = sqlite3_bind_double(statement, index, value)
            case let .blob(value):
                result = value.withUnsafeBytes { bytes in
                    sqlite3_bind_blob(statement, index, bytes.baseAddress, Int32(bytes.count), transient)
                }
            }
            guard result == SQLITE_OK else { throw AgentContextError.unavailable }
        }
    }

    private func transaction<T>(_ body: () throws -> T) throws -> T {
        try execute("BEGIN IMMEDIATE", [])
        do {
            let value = try body()
            try execute("COMMIT", [])
            return value
        } catch {
            try? execute("ROLLBACK", [])
            throw error
        }
    }

    private func text(_ statement: OpaquePointer, _ index: Int32) -> String {
        String(cString: sqlite3_column_text(statement, index))
    }

    private func blob(_ statement: OpaquePointer, _ index: Int32) -> Data {
        let count = Int(sqlite3_column_bytes(statement, index))
        guard count > 0, let pointer = sqlite3_column_blob(statement, index) else { return Data() }
        return Data(bytes: pointer, count: count)
    }
}
