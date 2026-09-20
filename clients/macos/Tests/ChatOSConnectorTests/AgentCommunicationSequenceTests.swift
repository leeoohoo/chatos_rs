import ChatOSConnector
import Foundation
import SQLite3
import XCTest

final class AgentCommunicationSequenceTests: XCTestCase {
    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-message-sequence-\(UUID().uuidString)")
            .appendingPathComponent("group-chat.db")
    }

    func testMultipleDocumentFreeMessagesCrossingLimitProduceOnePrivacySafeSignal() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let fingerprint = String(repeating: "a", count: 64)

        try await store.recordAgentMessageSent(
            ownerUserID: "owner-a",
            runFingerprint: fingerprint,
            characterCount: 1_200,
            documentCount: 0,
            nowUnixMs: 1_000
        )
        try await store.recordAgentMessageSent(
            ownerUserID: "owner-a",
            runFingerprint: fingerprint,
            characterCount: 900,
            documentCount: 0,
            nowUnixMs: 2_000
        )
        try await store.recordAgentMessageSent(
            ownerUserID: "owner-a",
            runFingerprint: fingerprint,
            characterCount: 100,
            documentCount: 0,
            nowUnixMs: 3_000
        )

        let snapshot = try await store.agentCommunicationMetricSnapshot(ownerUserID: "owner-a")
        let metric = try XCTUnwrap(snapshot.first { $0.name == "message_sequence" })
        XCTAssertEqual(metric.dimension, "possible_limit_bypass")
        XCTAssertEqual(metric.count, 1)
        XCTAssertEqual(metric.totalValue, 2)
        XCTAssertEqual(metric.maximumValue, 2)

        var database: OpaquePointer?
        XCTAssertEqual(sqlite3_open_v2(url.path, &database, SQLITE_OPEN_READONLY, nil), SQLITE_OK)
        defer { sqlite3_close(database) }
        var statement: OpaquePointer?
        XCTAssertEqual(
            sqlite3_prepare_v2(
                database,
                "SELECT run_fingerprint,character_count,document_count FROM local_agent_message_sequence_events ORDER BY sequence",
                -1,
                &statement,
                nil
            ),
            SQLITE_OK
        )
        defer { sqlite3_finalize(statement) }
        var rows = 0
        while sqlite3_step(statement) == SQLITE_ROW {
            rows += 1
            let fingerprintCString = try XCTUnwrap(sqlite3_column_text(statement, 0))
            XCTAssertEqual(String(cString: fingerprintCString), fingerprint)
            XCTAssertGreaterThan(sqlite3_column_int64(statement, 1), 0)
            XCTAssertEqual(sqlite3_column_int64(statement, 2), 0)
        }
        XCTAssertEqual(rows, 3)
    }

    func testDocumentOrExpiredWindowDoesNotFlagMessageSequence() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let withDocument = String(repeating: "b", count: 64)
        try await store.recordAgentMessageSent(
            ownerUserID: "owner-a",
            runFingerprint: withDocument,
            characterCount: 1_200,
            documentCount: 1,
            nowUnixMs: 1_000
        )
        try await store.recordAgentMessageSent(
            ownerUserID: "owner-a",
            runFingerprint: withDocument,
            characterCount: 900,
            documentCount: 0,
            nowUnixMs: 2_000
        )

        let expiredWindow = String(repeating: "c", count: 64)
        try await store.recordAgentMessageSent(
            ownerUserID: "owner-a",
            runFingerprint: expiredWindow,
            characterCount: 1_200,
            documentCount: 0,
            nowUnixMs: 1_000
        )
        try await store.recordAgentMessageSent(
            ownerUserID: "owner-a",
            runFingerprint: expiredWindow,
            characterCount: 900,
            documentCount: 0,
            nowUnixMs: 302_000
        )

        let snapshot = try await store.agentCommunicationMetricSnapshot(ownerUserID: "owner-a")
        XCTAssertFalse(snapshot.contains { $0.name == "message_sequence" })
    }
}
