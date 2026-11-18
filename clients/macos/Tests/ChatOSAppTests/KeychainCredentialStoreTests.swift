import Foundation
import Testing
@testable import ChatOSApp

@Suite("Authentication Credential Store")
struct KeychainCredentialStoreTests {
    @Test("stores tokens in Keychain without creating a plaintext file")
    func roundTripsToken() async throws {
        let fixture = fixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let store = KeychainCredentialStore(
            service: fixture.service,
            legacyCredentialURL: fixture.legacyURL
        )

        try await store.saveAccessToken(" token ")
        #expect(try await store.loadAccessToken() == "token")
        #expect(!FileManager.default.fileExists(atPath: fixture.legacyURL.path))
        try await store.deleteAccessToken()
    }

    @Test("migrates a plaintext token only after Keychain accepts it")
    func migratesLegacyToken() async throws {
        let fixture = fixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        try FileManager.default.createDirectory(at: fixture.root, withIntermediateDirectories: true)
        try Data("legacy-token".utf8).write(to: fixture.legacyURL)
        let store = KeychainCredentialStore(
            service: fixture.service,
            legacyCredentialURL: fixture.legacyURL
        )

        #expect(try await store.loadAccessToken() == "legacy-token")
        #expect(!FileManager.default.fileExists(atPath: fixture.legacyURL.path))
        try await store.deleteAccessToken()
    }

    private func fixture() -> (root: URL, legacyURL: URL, service: String) {
        let id = UUID().uuidString
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("auth-credential-\(id)", isDirectory: true)
        return (root, root.appendingPathComponent("access-token"), "com.chatos.tests.auth.\(id)")
    }
}
