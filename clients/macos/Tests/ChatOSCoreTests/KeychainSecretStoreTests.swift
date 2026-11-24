import Foundation
import Security
import Testing
@testable import ChatOSCore

@Suite("Keychain Secret Store")
struct KeychainSecretStoreTests {
    @Test("saves, updates, loads, and deletes a generic password")
    func roundTripsSecret() throws {
        let service = "com.chatos.tests.\(UUID().uuidString)"
        let store = KeychainSecretStore(service: service)
        let account = "credential"
        defer { try? store.delete(account: account) }

        #expect(try store.load(account: account) == nil)
        try store.save(Data("first".utf8), account: account)
        #expect(try store.load(account: account) == Data("first".utf8))
        try store.save(Data("second".utf8), account: account)
        #expect(try store.load(account: account) == Data("second".utf8))
        try store.delete(account: account)
        #expect(try store.load(account: account) == nil)
    }

    @Test("shares one vault item across credential services")
    func sharesOneVaultItem() throws {
        let id = UUID().uuidString
        let vaultService = "com.chatos.tests.vault.\(id)"
        let first = KeychainSecretStore(
            service: "com.chatos.tests.first.\(id)",
            vaultService: vaultService
        )
        let second = KeychainSecretStore(
            service: "com.chatos.tests.second.\(id)",
            vaultService: vaultService
        )
        defer {
            SecItemDelete([
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: vaultService,
            ] as CFDictionary)
        }

        try first.save(Data("first".utf8), account: "one")
        try second.save(Data("second".utf8), account: "two")

        #expect(try first.load(account: "one") == Data("first".utf8))
        #expect(try second.load(account: "two") == Data("second".utf8))
        var result: CFTypeRef?
        let status = SecItemCopyMatching([
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: vaultService,
            kSecReturnAttributes as String: true,
            kSecMatchLimit as String: kSecMatchLimitAll,
        ] as CFDictionary, &result)
        #expect(status == errSecSuccess)
        #expect((result as? [[String: Any]])?.count == 1)
    }

    @Test("legacy policy can defer old credential reads")
    func defersLegacyReads() throws {
        let id = UUID().uuidString
        let service = "com.chatos.tests.legacy.\(id)"
        let vaultService = "com.chatos.tests.vault.\(id)"
        let account = "credential"
        let legacyQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        defer {
            SecItemDelete(legacyQuery as CFDictionary)
            SecItemDelete([
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: vaultService,
            ] as CFDictionary)
        }
        #expect(SecItemAdd(legacyQuery.merging([
            kSecValueData as String: Data("legacy".utf8),
        ]) { _, value in value } as CFDictionary, nil) == errSecSuccess)

        let deferred = KeychainSecretStore(
            service: service,
            vaultService: vaultService,
            legacyItemPolicy: .none
        )
        #expect(try deferred.load(account: account) == nil)

        let migrating = KeychainSecretStore(
            service: service,
            vaultService: vaultService,
            legacyItemPolicy: .all
        )
        #expect(try migrating.load(account: account) == Data("legacy".utf8))
    }

    @Test("restores a legacy item over an incorrect vault replacement")
    func restoresLegacyItemOverReplacement() throws {
        let id = UUID().uuidString
        let service = "com.chatos.tests.restore-legacy.\(id)"
        let vaultService = "com.chatos.tests.vault.\(id)"
        let account = "device-signing-key-v1"
        let legacyQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let store = KeychainSecretStore(
            service: service,
            vaultService: vaultService,
            legacyItemPolicy: .all
        )
        defer {
            SecItemDelete(legacyQuery as CFDictionary)
            SecItemDelete([
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: vaultService,
            ] as CFDictionary)
        }

        try store.save(Data("replacement".utf8), account: account)
        #expect(SecItemAdd(legacyQuery.merging([
            kSecValueData as String: Data("original".utf8),
        ]) { _, value in value } as CFDictionary, nil) == errSecSuccess)

        #expect(try store.restoreLegacyItem(account: account) == Data("original".utf8))
        #expect(try store.load(account: account) == Data("original".utf8))
        #expect(SecItemCopyMatching(legacyQuery as CFDictionary, nil) == errSecItemNotFound)
    }
}
