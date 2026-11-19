@testable import ChatOSConnector
import ChatOSCore
import Foundation
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
}
