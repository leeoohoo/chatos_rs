@testable import ChatOSConnector
import ChatOSCore
import Foundation
import Security
import Testing

@Suite("Native Connector Secret Store")
struct NativeConnectorSecretStoreTests {
    @Test("explicit production path still uses the production vault")
    func recognizesExplicitProductionPath() {
        #expect(NativeConnectorSecretStore.usesProductionVault(
            rootURL: NativeConnectorSecretStore.productionRootURL
        ))
        #expect(!NativeConnectorSecretStore.usesProductionVault(
            rootURL: FileManager.default.temporaryDirectory
        ))
    }

    @Test("stores secrets in Keychain without creating plaintext files")
    func roundTripsSecret() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("connector-secret-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeConnectorSecretStore(rootURL: root)
        defer { try? store.delete(account: "account") }

        try store.save(Data("secret".utf8), account: "account")
        #expect(try store.load(account: "account") == Data("secret".utf8))
        #expect(!FileManager.default.fileExists(atPath: root.appendingPathComponent("account").path))
    }

    @Test("migrates a plaintext secret and removes the source file")
    func migratesLegacyFile() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("connector-secret-legacy-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let legacyURL = root.appendingPathComponent("account")
        try Data("legacy".utf8).write(to: legacyURL)
        let store = NativeConnectorSecretStore(rootURL: root)
        defer { try? store.delete(account: "account") }

        #expect(try store.load(account: "account") == Data("legacy".utf8))
        #expect(!FileManager.default.fileExists(atPath: legacyURL.path))
    }

    @Test("non-migrating reads leave legacy secrets untouched")
    func leavesLegacySecretForDeferredMigration() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("connector-secret-deferred-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeConnectorSecretStore(rootURL: root)
        let account = "remote-connection-credentials-v1:connection"
        let legacyURL = root.appendingPathComponent("remote-connection-credentials-v1_connection")
        try store.save(Data("current".utf8), account: account)
        try Data("legacy".utf8).write(to: legacyURL)
        defer { try? store.delete(account: account) }

        #expect(try store.load(account: account, migrateLegacy: false) == Data("current".utf8))
        #expect(FileManager.default.fileExists(atPath: legacyURL.path))
    }

    @Test("non-migrating reads do not open legacy Keychain items")
    func leavesLegacyKeychainItemForDeferredMigration() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("connector-secret-deferred-keychain-\(UUID().uuidString)", isDirectory: true)
        let account = "gateway-access-token-v1"
        let legacyQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: NativeConnectorSecretStore.keychainService(rootURL: root),
            kSecAttrAccount as String: account,
        ]
        let store = NativeConnectorSecretStore(rootURL: root)
        defer {
            try? store.delete(account: account)
            SecItemDelete(legacyQuery as CFDictionary)
            try? FileManager.default.removeItem(at: root)
        }

        #expect(SecItemAdd(legacyQuery.merging([
            kSecValueData as String: Data("legacy".utf8),
        ]) { _, value in value } as CFDictionary, nil) == errSecSuccess)

        #expect(try store.load(account: account, migrateLegacy: false) == nil)
        #expect(SecItemCopyMatching(legacyQuery as CFDictionary, nil) == errSecSuccess)
    }

    @Test("migrates the previous Keychain service and deletes the old item")
    func migratesLegacyKeychainItem() throws {
        let id = UUID().uuidString
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("connector-secret-keychain-\(id)", isDirectory: true)
        let legacy = KeychainSecretStore(service: "com.chatos.tests.connector-legacy.\(id)")
        try legacy.save(Data("legacy-keychain".utf8), account: "account")
        let store = NativeConnectorSecretStore(
            rootURL: root,
            migrateLegacyKeychain: true,
            legacyKeychainService: "com.chatos.tests.connector-legacy.\(id)"
        )
        defer {
            try? store.delete(account: "account")
            try? legacy.delete(account: "account")
            try? FileManager.default.removeItem(at: root)
        }

        #expect(try store.load(account: "account") == Data("legacy-keychain".utf8))
        #expect(try legacy.load(account: "account") == nil)
    }

    @Test("restores the original device key after a vault migration generated a replacement")
    func restoresOriginalDeviceKey() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("connector-device-key-\(UUID().uuidString)", isDirectory: true)
        let service = NativeConnectorSecretStore.keychainService(rootURL: root)
        let account = NativeConnectorSecretStore.deviceSigningKeyAccount
        let legacyQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let store = NativeConnectorSecretStore(rootURL: root)
        defer {
            try? store.delete(account: account)
            SecItemDelete(legacyQuery as CFDictionary)
            try? FileManager.default.removeItem(at: root)
        }

        try store.save(Data("replacement".utf8), account: account)
        #expect(SecItemAdd(legacyQuery.merging([
            kSecValueData as String: Data("original".utf8),
        ]) { _, value in value } as CFDictionary, nil) == errSecSuccess)

        #expect(try store.load(account: account) == Data("original".utf8))
        #expect(try store.load(account: account) == Data("original".utf8))
        #expect(SecItemCopyMatching(legacyQuery as CFDictionary, nil) == errSecItemNotFound)
    }
}
