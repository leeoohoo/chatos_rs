// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native Plugin state storage")
struct NativePluginStateStoreTests {
    @Test("installation Release and enablement use account-scoped Client Storage")
    func persistsInstallationAndEnablement() async throws {
        let transport = PluginStateTransport()
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)
        let store = NativePluginStateStore(
            accountSession: PluginStateAccountSession(client: client)
        )
        try await store.activate(ownerUserID: "user-1")
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "1.2.3",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: "/plugins/plugin-1/1.2.3",
            installedAt: "2026-09-14T02:00:00Z"
        )

        #expect(try await store.put(ownerUserID: "user-1", record: record) == record)
        try await store.setEnabled(ownerUserID: "user-1", pluginID: "plugin-1", enabled: false)
        #expect(try await store.record(ownerUserID: "user-1", pluginID: "plugin-1") == record)
        #expect(try await store.isEnabled(ownerUserID: "user-1", pluginID: "plugin-1") == false)

        await store.deactivate()
        let restored = NativePluginStateStore(
            accountSession: PluginStateAccountSession(client: client)
        )
        try await restored.activate(ownerUserID: "user-1")
        #expect(try await restored.record(ownerUserID: "user-1", pluginID: "plugin-1") == record)
        #expect(try await restored.isEnabled(ownerUserID: "user-1", pluginID: "plugin-1") == false)
        try await restored.remove(ownerUserID: "user-1", pluginID: "plugin-1")
        #expect(try await restored.record(ownerUserID: "user-1", pluginID: "plugin-1") == nil)
    }
}

private struct PluginStateAccountSession: NativeLocalAgentAccountSessionAccess {
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

private actor PluginStateTransport: LocalAgentFrameTransport {
    private var snapshot: [String: Any]?
    private var revision: UInt64 = 0

    func exchange(_ request: Data) async throws -> Data {
        let root = try Self.dictionary(JSONSerialization.jsonObject(with: request))
        let requestID = try Self.string(root["request_id"])
        let owner = try Self.string(root["owner_user_id"])
        let command = try Self.dictionary(root["command"])
        let type = try Self.string(command["type"])
        let payload = command["payload"] as? [String: Any] ?? [:]
        let response: [String: Any]
        switch type {
        case "list_installed_plugins":
            response = [
                "type": "installed_plugin_records",
                "payload": ["records": snapshot.map { [$0] } ?? [], "next_cursor": NSNull()],
            ]
        case "put_installed_plugin":
            revision += 1
            let draft = try Self.dictionary(payload["draft"])
            let value: [String: Any] = [
                "record_id": "installed-plugin:test",
                "owner_user_id": owner,
                "draft": draft,
                "revision": revision,
                "created_at": "2026-09-14T02:00:00Z",
                "updated_at": "2026-09-14T02:05:00Z",
            ]
            snapshot = value
            response = ["type": "installed_plugin", "payload": value]
        case "delete_installed_plugin":
            snapshot = nil
            response = ["type": "success"]
        default:
            throw PluginStateStoreTestError.unexpectedCommand(type)
        }
        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": requestID,
            "response": response,
        ])
    }

    private static func dictionary(_ value: Any?) throws -> [String: Any] {
        guard let value = value as? [String: Any] else {
            throw PluginStateStoreTestError.invalidRequest
        }
        return value
    }

    private static func string(_ value: Any?) throws -> String {
        guard let value = value as? String else {
            throw PluginStateStoreTestError.invalidRequest
        }
        return value
    }
}

private enum PluginStateStoreTestError: Error {
    case invalidRequest
    case unexpectedCommand(String)
}
