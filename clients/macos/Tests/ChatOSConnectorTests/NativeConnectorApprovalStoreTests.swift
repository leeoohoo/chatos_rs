// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native Connector approval storage")
struct NativeConnectorApprovalStoreTests {
    @Test("preferences and audit history use account-scoped Client Storage")
    func persistsPreferencesAndHistory() async throws {
        let transport = ApprovalStoreTransport()
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)
        let store = NativeConnectorApprovalStore(
            accountSession: ApprovalStoreAccountSession(client: client)
        )

        #expect(try await store.activate(ownerUserID: "user-1") == .defaults)
        var preferences = try await store.updateMode(
            ownerUserID: "user-1",
            mode: .autoApproval
        )
        preferences = try await store.updateModelSelection(
            ownerUserID: "user-1",
            modelConfigID: "model-1",
            thinkingLevel: "high"
        )
        #expect(preferences.defaultMode == .autoApproval)
        #expect(preferences.commandApprovalModelConfigID == "model-1")

        let entry = LocalConnectorApprovalHistoryEntry(
            id: "approval-1",
            command: "git push origin main",
            cwd: "/workspace/project",
            source: "native-terminal",
            mode: .autoApproval,
            decision: "approved",
            risk: "high",
            reason: "Approved by policy",
            createdAt: "2026-09-14T04:00:00Z"
        )
        #expect(try await store.append(ownerUserID: "user-1", entry: entry) == entry)
        #expect(try await store.history(ownerUserID: "user-1") == [entry])
        await store.deactivate()

        let restored = NativeConnectorApprovalStore(
            accountSession: ApprovalStoreAccountSession(client: client)
        )
        #expect(try await restored.activate(ownerUserID: "user-1") == preferences)
        #expect(try await restored.history(ownerUserID: "user-1") == [entry])
    }

    @Test("inactive approval storage fails closed")
    func failsClosedBeforeActivationAndAfterDeactivation() async throws {
        let client = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: ApprovalStoreTransport()
        )
        let store = NativeConnectorApprovalStore(
            accountSession: ApprovalStoreAccountSession(client: client)
        )
        await #expect(throws: NativeLocalClientSettingStoreError.notLoaded) {
            _ = try await store.history(ownerUserID: "user-1")
        }
        _ = try await store.activate(ownerUserID: "user-1")
        await store.deactivate()
        await #expect(throws: NativeLocalClientSettingStoreError.notLoaded) {
            _ = try await store.updateMode(ownerUserID: "user-1", mode: .fullControl)
        }
    }
}

private struct ApprovalStoreAccountSession: NativeLocalAgentAccountSessionAccess {
    let client: NativeLocalAgentIPCClient

    func client(accountID: String) async throws -> NativeLocalAgentIPCClient { client }
    func activeClient() async throws -> NativeLocalAgentIPCClient { client }
    func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID: String
    ) async throws -> [LocalAgentAttachmentReference] { [] }
    func discardStagedAttachments(
        _ references: [LocalAgentAttachmentReference],
        accountID: String
    ) async {}
}

private actor ApprovalStoreTransport: LocalAgentFrameTransport {
    private var preferenceValue: Any?
    private var preferenceRevision: UInt64 = 0
    private var history: [[String: Any]] = []

    func exchange(_ request: Data) async throws -> Data {
        let root = try Self.dictionary(JSONSerialization.jsonObject(with: request))
        let requestID = try Self.string(root["request_id"])
        let owner = try Self.string(root["owner_user_id"])
        let command = try Self.dictionary(root["command"])
        let type = try Self.string(command["type"])
        let payload = try Self.dictionary(command["payload"])
        let response: [String: Any]
        switch type {
        case "get_client_setting":
            let key = try Self.string(payload["key"])
            if let preferenceValue {
                response = Self.setting(
                    owner: owner,
                    key: key,
                    value: preferenceValue,
                    revision: preferenceRevision
                )
            } else {
                response = Self.error(code: "client_setting_not_found")
            }
        case "put_client_setting":
            let key = try Self.string(payload["key"])
            preferenceRevision += 1
            preferenceValue = payload["value"]
            response = Self.setting(
                owner: owner,
                key: key,
                value: try #require(preferenceValue),
                revision: preferenceRevision
            )
        case "append_approval_history":
            let recordID = try Self.string(payload["record_id"])
            let draft = try Self.dictionary(payload["draft"])
            let snapshot: [String: Any] = [
                "record_id": recordID,
                "owner_user_id": owner,
                "draft": draft,
                "revision": 1,
                "created_at": "2026-09-14T04:00:00Z",
                "updated_at": "2026-09-14T04:00:00Z",
            ]
            history.insert(snapshot, at: 0)
            response = ["type": "approval_history", "payload": snapshot]
        case "list_approval_history":
            response = [
                "type": "approval_history_records",
                "payload": ["records": history, "next_cursor": NSNull()],
            ]
        default:
            throw ApprovalStoreTestError.unexpectedCommand(type)
        }
        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": requestID,
            "response": response,
        ])
    }

    private static func setting(
        owner: String,
        key: String,
        value: Any,
        revision: UInt64
    ) -> [String: Any] {
        [
            "type": "client_setting",
            "payload": [
                "key": key,
                "owner_user_id": owner,
                "value": value,
                "revision": revision,
                "created_at": "2026-09-14T04:00:00Z",
                "updated_at": "2026-09-14T04:00:00Z",
            ],
        ]
    }

    private static func error(code: String) -> [String: Any] {
        [
            "type": "error",
            "payload": ["code": code, "message": "not found", "retryable": false],
        ]
    }

    private static func dictionary(_ value: Any?) throws -> [String: Any] {
        guard let value = value as? [String: Any] else {
            throw ApprovalStoreTestError.invalidRequest
        }
        return value
    }

    private static func string(_ value: Any?) throws -> String {
        guard let value = value as? String else {
            throw ApprovalStoreTestError.invalidRequest
        }
        return value
    }
}

private enum ApprovalStoreTestError: Error {
    case invalidRequest
    case unexpectedCommand(String)
}
