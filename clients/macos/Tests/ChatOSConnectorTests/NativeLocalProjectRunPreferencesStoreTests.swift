// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native local Project Run preferences")
struct NativeLocalProjectRunPreferencesStoreTests {
    @Test("typed Client Setting store commits only the newest account-scoped mutation")
    func typedStorePersistsWithoutAFileFallback() async throws {
        let transport = ProjectRunPreferencesTransport()
        let client = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: transport
        )
        let store = try NativeLocalClientSettingStore<TestClientPreference>(
            key: "test.preferences",
            accountSession: ProjectRunPreferencesAccountSession(client: client)
        )

        #expect(try await store.load(
            ownerUserID: "user-1",
            defaultValue: .init(enabled: false, label: "default")
        ) == .init(enabled: false, label: "default"))
        #expect(try await store.saveLatest(
            ownerUserID: "user-1",
            value: .init(enabled: true, label: "newest"),
            mutation: 2
        ))
        #expect(try await !store.saveLatest(
            ownerUserID: "user-1",
            value: .init(enabled: false, label: "late-old-value"),
            mutation: 1
        ))

        let olderWrite = Task {
            try await store.saveLatest(
                ownerUserID: "user-1",
                value: .init(enabled: false, label: "concurrent-old"),
                mutation: 3
            )
        }
        try await Task.sleep(for: .milliseconds(5))
        let newestWrite = Task {
            try await store.saveLatest(
                ownerUserID: "user-1",
                value: .init(enabled: true, label: "concurrent-newest"),
                mutation: 4
            )
        }
        #expect(try await olderWrite.value)
        #expect(try await newestWrite.value)

        await store.reset()
        #expect(try await store.load(
            ownerUserID: "user-1",
            defaultValue: .init(enabled: false, label: "unused")
        ) == .init(enabled: true, label: "concurrent-newest"))
    }

    @Test("typed Client Setting store rejects a client bound to another account")
    func typedStoreRejectsAccountMismatch() async throws {
        let client = try NativeLocalAgentIPCClient(
            ownerUserID: "user-2",
            transport: ProjectRunPreferencesTransport()
        )
        let store = try NativeLocalClientSettingStore<TestClientPreference>(
            key: "test.preferences",
            accountSession: ProjectRunPreferencesAccountSession(client: client)
        )

        await #expect(throws: NativeLocalClientSettingStoreError.accountMismatch) {
            try await store.load(
                ownerUserID: "user-1",
                defaultValue: .init(enabled: false, label: "default")
            )
        }
    }

    @Test("creates, restores and revision-updates one account-scoped setting")
    func persistsThroughTheSharedClientSettingRepository() async throws {
        let transport = ProjectRunPreferencesTransport()
        let userOneClient = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: transport
        )
        let userOneSession = ProjectRunPreferencesAccountSession(client: userOneClient)
        let firstStore = NativeLocalProjectRunPreferencesStore(accountSession: userOneSession)

        #expect(try await firstStore.selection(projectID: "project-1") == .init())
        let initial = NativeProjectRunSelection(
            defaultTargetID: "web",
            selectedToolchains: ["node": "/opt/node/bin/node"],
            customToolchains: [:],
            environmentVariables: ["APP_ENV": "development"]
        )
        try await firstStore.save(projectID: "project-1", selection: initial)

        let restoredStore = NativeLocalProjectRunPreferencesStore(accountSession: userOneSession)
        #expect(try await restoredStore.selection(projectID: "project-1") == initial)

        var updated = initial
        updated.defaultTargetID = "preview"
        updated.environmentVariables["APP_ENV"] = "preview"
        try await restoredStore.save(projectID: "project-1", selection: updated)

        let userTwoClient = try NativeLocalAgentIPCClient(
            ownerUserID: "user-2",
            transport: transport
        )
        let userTwoStore = NativeLocalProjectRunPreferencesStore(
            accountSession: ProjectRunPreferencesAccountSession(client: userTwoClient)
        )
        #expect(try await userTwoStore.selection(projectID: "project-1") == .init())

        let requests = try await transport.requests().map(Self.requestFields)
        #expect(requests.map(\.type) == [
            "get_client_setting",
            "put_client_setting",
            "get_client_setting",
            "put_client_setting",
            "get_client_setting",
        ])
        #expect(requests.map(\.owner) == ["user-1", "user-1", "user-1", "user-1", "user-2"])
        #expect(requests[1].expectedRevision == nil)
        #expect(requests[3].expectedRevision == 1)
    }

    private static func requestFields(_ data: Data) throws -> (
        type: String,
        owner: String,
        expectedRevision: UInt64?
    ) {
        let object = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let command = try #require(object["command"] as? [String: Any])
        let payload = try #require(command["payload"] as? [String: Any])
        return (
            type: try #require(command["type"] as? String),
            owner: try #require(object["owner_user_id"] as? String),
            expectedRevision: (payload["expected_revision"] as? NSNumber)?.uint64Value
        )
    }
}

private struct TestClientPreference: Codable, Equatable, Sendable {
    var enabled: Bool
    var label: String
}

private struct ProjectRunPreferencesAccountSession: NativeLocalAgentAccountSessionAccess {
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

private actor ProjectRunPreferencesTransport: LocalAgentFrameTransport {
    private struct StoredSetting {
        var value: Any
        var revision: UInt64
    }

    private var settings: [String: StoredSetting] = [:]
    private var recordedRequests: [Data] = []

    func exchange(_ request: Data) async throws -> Data {
        recordedRequests.append(request)
        let object = try Self.dictionary(JSONSerialization.jsonObject(with: request))
        let owner = try Self.string(object["owner_user_id"])
        let command = try Self.dictionary(object["command"])
        let type = try Self.string(command["type"])
        let payload = try Self.dictionary(command["payload"])
        let key = try Self.string(payload["key"])
        let storageKey = "\(owner):\(key)"
        let response: [String: Any]

        if type == "put_client_setting",
           let value = payload["value"] as? [String: Any],
           value["label"] as? String == "concurrent-old" {
            try? await Task.sleep(for: .milliseconds(50))
        }

        switch type {
        case "get_client_setting":
            if let setting = settings[storageKey] {
                response = Self.settingResponse(
                    owner: owner,
                    key: key,
                    value: setting.value,
                    revision: setting.revision
                )
            } else {
                response = Self.notFoundResponse()
            }
        case "put_client_setting":
            let expected = (payload["expected_revision"] as? NSNumber)?.uint64Value
            let current = settings[storageKey]
            guard current?.revision == expected else {
                response = [
                    "type": "error",
                    "payload": [
                        "code": "client_setting_revision_conflict",
                        "message": "revision conflict",
                        "retryable": false,
                    ],
                ]
                break
            }
            let revision = (current?.revision ?? 0) + 1
            let value = try #require(payload["value"])
            settings[storageKey] = StoredSetting(value: value, revision: revision)
            response = Self.settingResponse(
                owner: owner,
                key: key,
                value: value,
                revision: revision
            )
        default:
            throw ProjectRunPreferencesTestError.unexpectedCommand(type)
        }

        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": try Self.string(object["request_id"]),
            "response": response,
        ])
    }

    func requests() -> [Data] { recordedRequests }

    private static func settingResponse(
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
                "created_at": "2026-09-14T01:00:00Z",
                "updated_at": "2026-09-14T01:01:00Z",
            ],
        ]
    }

    private static func notFoundResponse() -> [String: Any] {
        [
            "type": "error",
            "payload": [
                "code": "client_setting_not_found",
                "message": "not found",
                "retryable": false,
            ],
        ]
    }

    private static func dictionary(_ value: Any?) throws -> [String: Any] {
        guard let value = value as? [String: Any] else {
            throw ProjectRunPreferencesTestError.invalidRequest
        }
        return value
    }

    private static func string(_ value: Any?) throws -> String {
        guard let value = value as? String else {
            throw ProjectRunPreferencesTestError.invalidRequest
        }
        return value
    }
}

private enum ProjectRunPreferencesTestError: Error {
    case invalidRequest
    case unexpectedCommand(String)
}
