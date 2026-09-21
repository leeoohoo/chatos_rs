import ChatOSConnector
import ChatOSCore
import Foundation
import SQLite3
import XCTest

final class AgentGroupChatHistoricalMigrationTests: XCTestCase {
    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-group-chat-history-\(UUID().uuidString)")
            .appendingPathComponent("group-chat.db")
    }

    private func installV11Fixture(at databaseURL: URL) throws {
        try FileManager.default.createDirectory(
            at: databaseURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let fixtureURL = try XCTUnwrap(
            Bundle.module.url(
                forResource: "AgentGroupChatSchemaV11",
                withExtension: "sql",
                subdirectory: "Fixtures"
            ) ?? Bundle.module.url(
                forResource: "AgentGroupChatSchemaV11",
                withExtension: "sql"
            )
        )
        let fixture = try String(contentsOf: fixtureURL, encoding: .utf8)
        var database: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &database,
            SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK, let database else {
            defer { sqlite3_close(database) }
            throw AgentGroupChatError.storage("fixture database open failed")
        }
        defer { sqlite3_close(database) }
        guard sqlite3_exec(database, fixture, nil, nil, nil) == SQLITE_OK else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
    }

    private func strings(_ databaseURL: URL, sql: String) throws -> [String] {
        var database: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &database,
            SQLITE_OPEN_READONLY | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK, let database else {
            defer { sqlite3_close(database) }
            throw AgentGroupChatError.storage("database open failed")
        }
        defer { sqlite3_close(database) }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
        defer { sqlite3_finalize(statement) }
        var result: [String] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let value = sqlite3_column_text(statement, 0) else { continue }
            result.append(String(cString: value))
        }
        return result
    }

    private func integer(_ databaseURL: URL, sql: String) throws -> Int64 {
        var database: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &database,
            SQLITE_OPEN_READONLY | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK, let database else {
            defer { sqlite3_close(database) }
            throw AgentGroupChatError.storage("database open failed")
        }
        defer { sqlite3_close(database) }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW else {
            throw AgentGroupChatError.storage("query returned no row")
        }
        return sqlite3_column_int64(statement, 0)
    }

    func testV11FixtureUpgradesToCurrentSchemaWithoutChangingHistoricalRows() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        try installV11Fixture(at: url)

        var store: SQLiteAgentGroupChatStore? = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agents = try await store!.listAgents(ownerUserID: "fixture-owner")
        XCTAssertEqual(agents.map(\.id), ["fixture-agent"])
        XCTAssertEqual(agents.first?.draft.rolePrompt, "Preserve this role.")
        XCTAssertEqual(agents.first?.draft.thinkingLevel, "medium")
        XCTAssertEqual(agents.first?.draft.heartbeatEnabled, false)
        XCTAssertEqual(agents.first?.draft.heartbeatIntervalSeconds, 900)

        let rooms = try await store!.listRooms(ownerUserID: "fixture-owner")
        XCTAssertEqual(rooms.map(\.id), ["fixture-room"])
        XCTAssertEqual(rooms.first?.draft.goal, "Preserve this goal.")
        XCTAssertNil(rooms.first?.projectManagerAgentID)

        let messages = try await store!.listMessages(
            ownerUserID: "fixture-owner",
            roomID: "fixture-room",
            limit: 20
        )
        XCTAssertEqual(messages.map(\.id), ["fixture-message"])
        XCTAssertEqual(messages.first?.content, "Preserve this message.")
        let attachment = try XCTUnwrap(messages.first?.attachmentItems.first)
        XCTAssertEqual(attachment.id, "fixture-attachment")
        XCTAssertEqual(attachment.syncStatus, .localOnly)
        XCTAssertNil(attachment.artifactID)

        let delivery = try await store!.delivery(
            ownerUserID: "fixture-owner",
            deliveryID: "fixture-delivery"
        )
        XCTAssertEqual(delivery?.status, .pending)
        XCTAssertEqual(delivery?.triggerKind, .mention)

        try await store!.recordAgentMessageAttempt(
            ownerUserID: "fixture-owner",
            characterCount: 42,
            rejected: false,
            nowUnixMs: 200
        )
        let metrics = try await store!.agentCommunicationMetricSnapshot(
            ownerUserID: "fixture-owner"
        )
        XCTAssertEqual(metrics.count, 1)
        store = nil

        XCTAssertEqual(
            try strings(
                url,
                sql: "SELECT CAST(version AS TEXT) FROM local_agent_group_chat_schema_migrations ORDER BY version"
            ),
            (1...26).map(String.init)
        )
        XCTAssertEqual(
            try strings(
                url,
                sql: "SELECT name FROM pragma_table_info('local_agent_profiles') WHERE name = 'avatar_data'"
            ),
            ["avatar_data"]
        )
        XCTAssertEqual(
            try strings(
                url,
                sql: "SELECT name FROM pragma_table_info('project_agent_message_attachments') ORDER BY cid"
            ),
            [
                "owner_user_id", "message_id", "id", "position", "name", "mime_type",
                "size_bytes", "kind", "origin", "relative_path", "sha256", "sync_status",
                "artifact_id", "storage_provider", "bucket", "object_key", "remote_view_path",
                "upload_error", "synced_at_unix_ms", "upload_attempt",
                "next_retry_at_unix_ms",
            ]
        )
        XCTAssertEqual(
            try integer(url, sql: "SELECT COUNT(*) FROM local_agent_group_chat_runs"),
            1
        )
        XCTAssertEqual(try integer(url, sql: "SELECT COUNT(*) FROM pragma_foreign_key_check"), 0)

        let reopened = try SQLiteAgentGroupChatStore(databaseURL: url)
        let reopenedMessages = try await reopened.listMessages(
            ownerUserID: "fixture-owner",
            roomID: "fixture-room",
            limit: 20
        )
        XCTAssertEqual(reopenedMessages, messages)
    }
}
