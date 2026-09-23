import ChatOSConnector
import ChatOSCore
import Foundation
import SQLite3
import XCTest

final class AgentCommunicationMetricsTests: XCTestCase {
    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-communication-metrics-\(UUID().uuidString)")
            .appendingPathComponent("group-chat.db")
    }

    private func executeSQLite(_ databaseURL: URL, sql: String) throws {
        var database: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &database,
            SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK, let database else {
            defer { sqlite3_close(database) }
            throw NSError(domain: "AgentCommunicationMetricsTests", code: 1)
        }
        defer { sqlite3_close(database) }
        guard sqlite3_exec(database, sql, nil, nil, nil) == SQLITE_OK else {
            throw NSError(
                domain: "AgentCommunicationMetricsTests",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: String(cString: sqlite3_errmsg(database))]
            )
        }
    }

    private func metric(
        _ snapshot: [AgentCommunicationMetricRow],
        name: String,
        dimension: String
    ) throws -> AgentCommunicationMetricRow {
        try XCTUnwrap(snapshot.first { $0.name == name && $0.dimension == dimension })
    }

    func testMigration24UpgradesExistingDatabaseAndPreservesMetrics() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        var initialStore: SQLiteAgentGroupChatStore? = try SQLiteAgentGroupChatStore(databaseURL: url)
        XCTAssertNotNil(initialStore)
        initialStore = nil

        try executeSQLite(
            url,
            sql: """
            DROP TABLE local_agent_communication_metrics;
            DELETE FROM local_agent_group_chat_schema_migrations WHERE version = 24;
            """
        )

        let migratedStore = try SQLiteAgentGroupChatStore(databaseURL: url)
        try await migratedStore.recordAgentToolRejection(
            ownerUserID: "owner-a",
            reason: .messageTooLong,
            nowUnixMs: 100
        )

        let snapshot = try await migratedStore.agentCommunicationMetricSnapshot(
            ownerUserID: "owner-a"
        )
        XCTAssertEqual(snapshot, [
            .init(
                name: "tool_rejection",
                dimension: "message_too_long",
                count: 1,
                totalValue: 1,
                maximumValue: 1,
                updatedAtUnixMs: 100
            ),
        ])
    }

    func testMessageLengthBucketsIncludeBoundariesAndAggregateValues() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let policy = AgentCommunicationPolicy.standard
        let concise = policy.conciseMessageCharacters
        let recommended = policy.recommendedMessageCharacters
        let maximum = policy.maximumMessageCharacters

        for (index, length) in [
            concise, concise + 1, recommended, recommended + 1, maximum, maximum + 1, 500,
        ].enumerated() {
            try await store.recordAgentMessageAttempt(
                ownerUserID: "owner-a",
                characterCount: length,
                rejected: length > maximum,
                nowUnixMs: Int64(index + 1)
            )
        }

        let snapshot = try await store.agentCommunicationMetricSnapshot(ownerUserID: "owner-a")
        let short = try metric(snapshot, name: "message_length", dimension: "concise")
        let recommendedBucket = try metric(
            snapshot,
            name: "message_length",
            dimension: "recommended"
        )
        let long = try metric(snapshot, name: "message_length", dimension: "extended")
        let rejected = try metric(snapshot, name: "message_length", dimension: "over_limit")

        XCTAssertEqual(short.count, 1)
        XCTAssertEqual(short.totalValue, Int64(concise))
        XCTAssertEqual(recommendedBucket.count, 3)
        XCTAssertEqual(recommendedBucket.totalValue, Int64(concise + 1 + recommended + 500))
        XCTAssertEqual(recommendedBucket.maximumValue, Int64(recommended))
        XCTAssertEqual(long.count, 2)
        XCTAssertEqual(long.totalValue, Int64(recommended + 1 + maximum))
        XCTAssertEqual(long.maximumValue, Int64(maximum))
        XCTAssertEqual(rejected.count, 1)
        XCTAssertEqual(rejected.totalValue, Int64(maximum + 1))
        XCTAssertEqual(
            try metric(snapshot, name: "tool_rejection", dimension: "message_too_long").count,
            1
        )
    }

    func testDocumentUploadPreviewAndRejectionMetricsRemainPrivacySafe() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)

        try await store.recordAgentDocumentCreation(
            ownerUserID: "owner-a",
            outcome: .succeeded,
            bytes: 1_024,
            nowUnixMs: 10
        )
        try await store.recordAgentDocumentCreation(
            ownerUserID: "owner-a",
            outcome: .succeeded,
            bytes: 2_048,
            nowUnixMs: 11
        )
        try await store.recordAgentDocumentCreation(
            ownerUserID: "owner-a",
            outcome: .tooLarge,
            bytes: 3_000_000,
            nowUnixMs: 12
        )
        try await store.recordAgentArtifactUpload(
            ownerUserID: "owner-a",
            outcome: .succeeded,
            bytes: 1_024,
            nowUnixMs: 20
        )
        try await store.recordAgentArtifactUpload(
            ownerUserID: "owner-a",
            outcome: .failed,
            bytes: 2_048,
            nowUnixMs: 21
        )
        try await store.recordAgentDocumentPreview(
            ownerUserID: "owner-a",
            outcome: .succeeded,
            durationMilliseconds: 25,
            nowUnixMs: 30
        )
        try await store.recordAgentDocumentPreview(
            ownerUserID: "owner-a",
            outcome: .succeeded,
            durationMilliseconds: 75,
            nowUnixMs: 31
        )
        try await store.recordAgentDocumentPreview(
            ownerUserID: "owner-a",
            outcome: .failed,
            durationMilliseconds: 40,
            nowUnixMs: 32
        )
        for reason in [
            AgentCommunicationRejectionMetricReason.tooManyDocumentRefs,
            .duplicateDocumentRef,
            .invalidDocumentRef,
            .documentIntegrityChanged,
        ] {
            try await store.recordAgentToolRejection(
                ownerUserID: "owner-a",
                reason: reason,
                nowUnixMs: 40
            )
        }
        try await store.recordAgentToolRejection(
            ownerUserID: "owner-b",
            reason: .messageTooLong,
            nowUnixMs: 50
        )

        let snapshot = try await store.agentCommunicationMetricSnapshot(ownerUserID: "owner-a")
        let created = try metric(snapshot, name: "document_create", dimension: "succeeded")
        XCTAssertEqual(created.count, 2)
        XCTAssertEqual(created.totalValue, 3_072)
        XCTAssertEqual(created.maximumValue, 2_048)
        XCTAssertEqual(
            try metric(snapshot, name: "document_create", dimension: "too_large").count,
            1
        )
        XCTAssertEqual(
            try metric(snapshot, name: "artifact_upload", dimension: "succeeded").totalValue,
            1_024
        )
        XCTAssertEqual(
            try metric(snapshot, name: "artifact_upload", dimension: "failed").totalValue,
            2_048
        )
        let previewed = try metric(snapshot, name: "document_preview", dimension: "succeeded")
        XCTAssertEqual(previewed.count, 2)
        XCTAssertEqual(previewed.totalValue, 100)
        XCTAssertEqual(previewed.maximumValue, 75)
        XCTAssertEqual(
            try metric(snapshot, name: "document_preview", dimension: "failed").totalValue,
            40
        )

        let allowedNames = Set(["document_create", "artifact_upload", "document_preview", "tool_rejection"])
        let allowedDimensions = Set(
            AgentDocumentCreationMetricOutcome.allMetricValues
                + AgentArtifactUploadMetricOutcome.allMetricValues
                + AgentDocumentPreviewMetricOutcome.allMetricValues
                + AgentCommunicationRejectionMetricReason.allMetricValues
        )
        XCTAssertTrue(snapshot.allSatisfy { allowedNames.contains($0.name) })
        XCTAssertTrue(snapshot.allSatisfy { allowedDimensions.contains($0.dimension) })
        XCTAssertFalse(snapshot.contains { $0.dimension == "message_too_long" })
    }
}

private extension AgentDocumentCreationMetricOutcome {
    static let allMetricValues = [
        succeeded, empty, tooLarge, tooMany, runLimitExceeded, invalidName, invalidTitle,
        storageFailed,
    ].map(\.rawValue)
}

private extension AgentArtifactUploadMetricOutcome {
    static let allMetricValues = [succeeded, failed].map(\.rawValue)
}

private extension AgentDocumentPreviewMetricOutcome {
    static let allMetricValues = [succeeded, failed].map(\.rawValue)
}

private extension AgentCommunicationRejectionMetricReason {
    static let allMetricValues = [
        messageTooLong, tooManyDocumentRefs, duplicateDocumentRef, invalidDocumentRef,
        documentIntegrityChanged,
    ].map(\.rawValue)
}
