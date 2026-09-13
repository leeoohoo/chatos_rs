@testable import ChatOSApp
import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Client Setting-backed macOS preferences")
@MainActor
struct ClientSettingBackedPreferencesTests {
    @Test("Pet preferences persist through the selected provider and remain account scoped")
    func petPreferencesAreProviderOwned() async throws {
        let context = try PreferenceTestContext()
        let first = PetPreferencesStore(accountSession: context.session)
        await first.activate(ownerUserID: "user-1")
        #expect(first.isStorageReady)
        #expect(first.isEnabled)

        first.isEnabled = false
        first.size = 152
        first.setFavorite(true, projectID: "project-1")
        await first.flush()

        let restored = PetPreferencesStore(accountSession: context.session)
        await restored.activate(ownerUserID: "user-1")
        #expect(!restored.isEnabled)
        #expect(restored.size == 152)
        #expect(restored.isFavorite(projectID: "project-1"))

        await restored.activate(ownerUserID: "user-2")
        #expect(restored.isEnabled)
        #expect(restored.size == 104)
        #expect(!restored.isFavorite(projectID: "project-1"))
    }

    @Test("Global utility preferences persist one typed state without UserDefaults")
    func globalUtilitiesAreProviderOwned() async throws {
        let context = try PreferenceTestContext()
        let first = GlobalUtilityPreferencesStore(accountSession: context.session)
        await first.activate(ownerUserID: "user-1")
        first.isEnabled = true
        first.setActionEnabled(false, for: .screenshot)
        let replacement = GlobalHotKey(
            keyCode: 49,
            keyEquivalent: "Space",
            modifiers: [.option]
        )
        first.setHotKey(replacement, for: .quickSearch)
        await first.flush()

        let restored = GlobalUtilityPreferencesStore(accountSession: context.session)
        await restored.activate(ownerUserID: "user-1")
        #expect(restored.isStorageReady)
        #expect(restored.isEnabled)
        #expect(!restored.screenshotEnabled)
        #expect(restored.hotKey(for: .quickSearch) == replacement)
    }

    @Test("Quick Search usage is bounded by the selected account-scoped provider")
    func quickSearchUsageIsProviderOwned() async throws {
        let context = try PreferenceTestContext()
        let first = QuickSearchUsageStore(accountSession: context.session)
        await first.activate(ownerUserID: "user-1")
        first.recordUsage("project:one", now: 1_000)
        first.recordUsage("project:one", now: 2_000)
        await first.flush()

        let restored = QuickSearchUsageStore(accountSession: context.session)
        await restored.activate(ownerUserID: "user-1")
        let boost = restored.usageBoost(for: "project:one", now: 2_000)
        #expect(boost.recency == 70)
        #expect(boost.frequency > 0)

        await restored.activate(ownerUserID: "user-2")
        let isolated = restored.usageBoost(for: "project:one", now: 2_000)
        #expect(isolated.recency == 0)
        #expect(isolated.frequency == 0)
    }
}

private struct PreferenceTestContext {
    let session: PreferenceAccountSession

    init() throws {
        let transport = PreferenceTransport()
        session = PreferenceAccountSession(clients: [
            "user-1": try NativeLocalAgentIPCClient(
                ownerUserID: "user-1",
                transport: transport
            ),
            "user-2": try NativeLocalAgentIPCClient(
                ownerUserID: "user-2",
                transport: transport
            ),
        ])
    }
}

private struct PreferenceAccountSession: NativeLocalAgentAccountSessionAccess {
    let clients: [String: NativeLocalAgentIPCClient]

    func client(accountID: String) async throws -> NativeLocalAgentIPCClient {
        guard let client = clients[accountID] else {
            throw NativeLocalAgentAccountSessionError.accountMismatch
        }
        return client
    }

    func activeClient() async throws -> NativeLocalAgentIPCClient {
        guard let client = clients["user-1"] else {
            throw NativeLocalAgentAccountSessionError.inactive
        }
        return client
    }

    func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID: String
    ) async throws -> [LocalAgentAttachmentReference] { [] }

    func discardStagedAttachments(
        _ references: [LocalAgentAttachmentReference],
        accountID: String
    ) async {}
}

private actor PreferenceTransport: LocalAgentFrameTransport {
    private struct StoredSetting {
        var value: Any
        var revision: UInt64
    }

    private var settings: [String: StoredSetting] = [:]

    func exchange(_ request: Data) async throws -> Data {
        let envelope = try Self.dictionary(JSONSerialization.jsonObject(with: request))
        let owner = try Self.string(envelope["owner_user_id"])
        let command = try Self.dictionary(envelope["command"])
        let type = try Self.string(command["type"])
        let payload = try Self.dictionary(command["payload"])
        let key = try Self.string(payload["key"])
        let storageKey = "\(owner):\(key)"
        let response: [String: Any]

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
                response = [
                    "type": "error",
                    "payload": [
                        "code": "client_setting_not_found",
                        "message": "not found",
                        "retryable": false,
                    ],
                ]
            }
        case "put_client_setting":
            let expected = (payload["expected_revision"] as? NSNumber)?.uint64Value
            let current = settings[storageKey]
            guard current?.revision == expected else {
                throw PreferenceTestError.revisionConflict
            }
            let revision = (current?.revision ?? 0) + 1
            let value = try Self.value(payload["value"])
            settings[storageKey] = StoredSetting(value: value, revision: revision)
            response = Self.settingResponse(
                owner: owner,
                key: key,
                value: value,
                revision: revision
            )
        default:
            throw PreferenceTestError.unexpectedCommand
        }

        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": try Self.string(envelope["request_id"]),
            "response": response,
        ])
    }

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
                "created_at": "2026-09-14T02:00:00Z",
                "updated_at": "2026-09-14T02:01:00Z",
            ],
        ]
    }

    private static func dictionary(_ value: Any?) throws -> [String: Any] {
        guard let value = value as? [String: Any] else {
            throw PreferenceTestError.invalidFrame
        }
        return value
    }

    private static func string(_ value: Any?) throws -> String {
        guard let value = value as? String else {
            throw PreferenceTestError.invalidFrame
        }
        return value
    }

    private static func value(_ value: Any?) throws -> Any {
        guard let value else { throw PreferenceTestError.invalidFrame }
        return value
    }
}

private enum PreferenceTestError: Error {
    case invalidFrame
    case revisionConflict
    case unexpectedCommand
}
