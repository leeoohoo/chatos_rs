import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest
@testable import ChatOSConnector

final class DurableAgentMemoryServiceTests: XCTestCase {
    func testOfflineRecordsRemainComposableAndFlushAfterRestart() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let scope = try AgentMemoryScope(
            tenantID: "owner-a",
            agentID: "agent-a",
            projectID: "project-a",
            runID: UUID(),
            runtimeScope: "manager-a"
        )
        let store = try SQLiteAgentMemoryCacheStore(
            databaseURL: root.appendingPathComponent("memory.sqlite3")
        )
        let offline = DurableAgentMemoryService(
            scope: scope,
            store: store,
            remoteFactory: { OfflineAgentMemory() }
        )
        let entries = [
            AgentMemoryEntry(
                id: scope.recordID(at: 0),
                index: 0,
                message: .init(role: .system, content: "系统约束"),
                createdAt: Date(timeIntervalSince1970: 1)
            ),
            AgentMemoryEntry(
                id: scope.recordID(at: 1),
                index: 1,
                message: .init(role: .user, content: "离线消息"),
                createdAt: Date(timeIntervalSince1970: 2)
            ),
        ]

        try await offline.ensureThread()
        try await offline.sync(entries, reconciling: false)
        let localContext = try await offline.compose()
        XCTAssertEqual(localContext.recentRecords.map(\.message.content), ["系统约束", "离线消息"])

        // Simulate the shared connectivity gate reopening after its backoff.
        await store.recordRemoteSuccess()
        let remote = RecordingAgentMemory()
        let recovered = DurableAgentMemoryService(
            scope: scope,
            store: store,
            remoteFactory: { remote }
        )
        try await recovered.ensureThread()
        let uploaded = await remote.entries
        XCTAssertEqual(uploaded.map(\.id), entries.map(\.id))
        let recoveredContext = try await recovered.compose()
        XCTAssertEqual(recoveredContext.recentRecords.map(\.message.content), ["系统约束", "离线消息"])
        let syncCalls = await remote.syncCalls
        XCTAssertEqual(syncCalls, 1)
    }

    func testCacheRejectsARecordIDWithDifferentContent() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let scope = try AgentMemoryScope(
            tenantID: "owner-a", agentID: "agent-a", projectID: "project-a",
            runID: UUID(), runtimeScope: "manager-a"
        )
        let store = try SQLiteAgentMemoryCacheStore(
            databaseURL: root.appendingPathComponent("memory.sqlite3")
        )
        let first = AgentMemoryEntry(
            id: scope.recordID(at: 0), index: 0,
            message: .init(role: .user, content: "原始内容"), createdAt: Date(timeIntervalSince1970: 1)
        )
        try await store.enqueue([first], scope: scope)
        let changed = AgentMemoryEntry(
            id: first.id, index: 0,
            message: .init(role: .user, content: "被修改"), createdAt: first.createdAt
        )
        do {
            try await store.enqueue([changed], scope: scope)
            XCTFail("Expected immutable record validation")
        } catch AgentContextError.invalidHistory {
            // Expected.
        }
    }

    func testCacheAcceptsAnEquivalentRecordWithDifferentJSONFormatting() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let databaseURL = root.appendingPathComponent("memory.sqlite3")
        let scope = try AgentMemoryScope(
            tenantID: "owner-a", agentID: "agent-a", projectID: "project-a",
            runID: UUID(), runtimeScope: "manager-a"
        )
        let store = try SQLiteAgentMemoryCacheStore(databaseURL: databaseURL)
        let entry = AgentMemoryEntry(
            id: scope.recordID(at: 0), index: 0,
            message: .init(role: .tool, content: #"{"ok":true}"#, toolCallID: "call-1"),
            // Checkpoint epochs are decoded relative to Apple's reference date, while the cache
            // JSON strategy round-trips through Unix milliseconds. The two paths can differ by
            // one floating-point ULP even though they represent the same millisecond.
            createdAt: Date(timeIntervalSinceReferenceDate: 811_581_748.966_327)
        )

        try await store.enqueue([entry], scope: scope)
        try rewriteStoredEntryWithEquivalentPrettyPrintedJSON(
            databaseURL: databaseURL,
            threadID: scope.threadID,
            recordID: entry.id
        )

        try await store.enqueue([entry], scope: scope)
    }
}

private func rewriteStoredEntryWithEquivalentPrettyPrintedJSON(
    databaseURL: URL,
    threadID: String,
    recordID: String
) throws {
    var database: OpaquePointer?
    XCTAssertEqual(sqlite3_open_v2(databaseURL.path, &database, SQLITE_OPEN_READWRITE, nil), SQLITE_OK)
    guard let database else { throw AgentContextError.unavailable }
    defer { sqlite3_close(database) }

    var select: OpaquePointer?
    XCTAssertEqual(
        sqlite3_prepare_v2(
            database,
            "SELECT entry_json FROM agent_memory_records WHERE thread_id = ? AND record_id = ?",
            -1,
            &select,
            nil
        ),
        SQLITE_OK
    )
    guard let select else { throw AgentContextError.unavailable }
    defer { sqlite3_finalize(select) }
    let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
    sqlite3_bind_text(select, 1, threadID, -1, transient)
    sqlite3_bind_text(select, 2, recordID, -1, transient)
    guard sqlite3_step(select) == SQLITE_ROW,
          let bytes = sqlite3_column_blob(select, 0) else {
        throw AgentContextError.unavailable
    }
    let original = Data(bytes: bytes, count: Int(sqlite3_column_bytes(select, 0)))
    let object = try JSONSerialization.jsonObject(with: original)
    let reformatted = try JSONSerialization.data(
        withJSONObject: object,
        options: [.prettyPrinted, .sortedKeys]
    )
    XCTAssertNotEqual(original, reformatted)

    var update: OpaquePointer?
    XCTAssertEqual(
        sqlite3_prepare_v2(
            database,
            "UPDATE agent_memory_records SET entry_json = ? WHERE thread_id = ? AND record_id = ?",
            -1,
            &update,
            nil
        ),
        SQLITE_OK
    )
    guard let update else { throw AgentContextError.unavailable }
    defer { sqlite3_finalize(update) }
    _ = reformatted.withUnsafeBytes { buffer in
        sqlite3_bind_blob(update, 1, buffer.baseAddress, Int32(buffer.count), transient)
    }
    sqlite3_bind_text(update, 2, threadID, -1, transient)
    sqlite3_bind_text(update, 3, recordID, -1, transient)
    guard sqlite3_step(update) == SQLITE_DONE else { throw AgentContextError.unavailable }
}

private struct OfflineAgentMemory: AgentMemoryServicing {
    func ensureThread() async throws { throw URLError(.notConnectedToInternet) }
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws {
        throw URLError(.notConnectedToInternet)
    }
    func compose() async throws -> AgentMemoryContext {
        throw URLError(.notConnectedToInternet)
    }
}

private actor RecordingAgentMemory: AgentMemoryServicing {
    var entries: [AgentMemoryEntry] = []
    var syncCalls = 0

    func ensureThread() async throws {}
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws {
        syncCalls += 1
        self.entries.append(contentsOf: entries)
    }
    func compose() async throws -> AgentMemoryContext {
        .init(
            blocks: [],
            recentRecords: entries.map { .init(id: $0.id, message: $0.message) }
        )
    }
}
