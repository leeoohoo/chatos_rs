import ChatOSAgentRuntime
import Foundation
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
