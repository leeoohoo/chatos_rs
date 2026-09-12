import Foundation
import Testing
@testable import ChatOSApp

@Suite("Authentication Keychain", .serialized)
struct KeychainCredentialStoreTests {
    @Test("access token is stored, replaced and removed through Keychain")
    func keychainRoundTrip() async throws {
        let service = "com.chatos.tests.authentication.\(UUID().uuidString)"
        let account = "access-token"
        let writer = KeychainCredentialStore(service: service, account: account)
        do {
            #expect(try await writer.loadAccessToken() == nil)
            try await writer.saveAccessToken("first-token")
            #expect(try await writer.loadAccessToken() == "first-token")

            let independentReader = KeychainCredentialStore(service: service, account: account)
            #expect(try await independentReader.loadAccessToken() == "first-token")
            try await writer.saveAccessToken("second-token")

            let replacedReader = KeychainCredentialStore(service: service, account: account)
            #expect(try await replacedReader.loadAccessToken() == "second-token")
            try await writer.deleteAccessToken()

            let deletedReader = KeychainCredentialStore(service: service, account: account)
            #expect(try await deletedReader.loadAccessToken() == nil)
        } catch {
            try? await writer.deleteAccessToken()
            throw error
        }
        try? await writer.deleteAccessToken()
    }
}
