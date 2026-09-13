// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native terminal history store")
struct NativeTerminalHistoryStoreTests {
    @Test("persists lists and clears through the account-scoped Host")
    func persistsListsAndClears() async throws {
        let transport = TerminalHistoryTransport()
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)
        let store = NativeTerminalHistoryStore(
            accountSession: TerminalHistoryAccountSession(client: client)
        )
        let entry = LocalConnectorCommandHistoryEntry(
            id: "terminal:1",
            source: "native-terminal",
            workspaceAlias: "workspace",
            cwd: "/workspace",
            display: "cargo test",
            status: "completed",
            exitCode: 0,
            stdoutPreview: "ok",
            stderrPreview: nil,
            error: nil,
            startedAt: "2026-09-14T10:00:00Z"
        )

        #expect(try await store.append(ownerUserID: "user-1", entry: entry) == entry)
        #expect(try await store.list(ownerUserID: "user-1", limit: 10) == [entry])
        try await store.clear(ownerUserID: "user-1")
        #expect(try await store.list(ownerUserID: "user-1", limit: 10).isEmpty)
        #expect(await transport.commandTypes() == [
            "append_terminal_history",
            "list_terminal_history",
            "clear_terminal_history",
            "list_terminal_history",
        ])
    }

    @Test("rejects a Host client from another account")
    func rejectsAccountMismatch() async throws {
        let client = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: TerminalHistoryTransport()
        )
        let store = NativeTerminalHistoryStore(
            accountSession: TerminalHistoryAccountSession(client: client)
        )
        await #expect(throws: NativeTerminalHistoryStoreError.invalidOwner) {
            _ = try await store.list(ownerUserID: "user-2", limit: 10)
        }
    }
}

private struct TerminalHistoryAccountSession: NativeLocalAgentAccountSessionAccess {
    let clientValue: NativeLocalAgentIPCClient

    init(client: NativeLocalAgentIPCClient) {
        clientValue = client
    }

    func client(accountID _: String) async throws -> NativeLocalAgentIPCClient { clientValue }
    func activeClient() async throws -> NativeLocalAgentIPCClient { clientValue }

    func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID _: String
    ) async throws -> [LocalAgentAttachmentReference] {
        guard attachments.isEmpty else { throw NativeLocalAgentAccountSessionError.inactive }
        return []
    }

    func discardStagedAttachments(
        _: [LocalAgentAttachmentReference],
        accountID _: String
    ) async {}
}

private actor TerminalHistoryTransport: LocalAgentFrameTransport {
    private var records: [[String: Any]] = []
    private var commands: [String] = []

    func exchange(_ request: Data) async throws -> Data {
        let object = try #require(
            JSONSerialization.jsonObject(with: request) as? [String: Any]
        )
        let requestID = try #require(object["request_id"] as? String)
        let command = try #require(object["command"] as? [String: Any])
        let type = try #require(command["type"] as? String)
        commands.append(type)
        let response: [String: Any]
        switch type {
        case "append_terminal_history":
            let payload = try #require(command["payload"] as? [String: Any])
            let record: [String: Any] = [
                "record_id": try #require(payload["record_id"] as? String),
                "owner_user_id": "user-1",
                "draft": try #require(payload["draft"] as? [String: Any]),
                "revision": 1,
                "created_at": "2026-09-14T10:00:00Z",
                "updated_at": "2026-09-14T10:00:00Z",
            ]
            records.append(record)
            response = ["type": "terminal_history", "payload": record]
        case "list_terminal_history":
            response = [
                "type": "terminal_history_records",
                "payload": ["records": records, "next_cursor": NSNull()],
            ]
        case "clear_terminal_history":
            records.removeAll()
            response = ["type": "success"]
        default:
            Issue.record("Unexpected terminal history command: \(type)")
            response = ["type": "error", "payload": [
                "code": "unexpected",
                "message": "unexpected command",
                "retryable": false,
            ]]
        }
        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": requestID,
            "response": response,
        ])
    }

    func commandTypes() -> [String] { commands }
}
