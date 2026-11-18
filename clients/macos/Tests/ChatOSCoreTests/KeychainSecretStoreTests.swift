import Foundation
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
}
