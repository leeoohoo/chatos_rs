import Foundation
import Testing
import ChatOSConnector
@testable import ChatOSApp

@Suite("Authentication Keychain", .serialized)
struct KeychainCredentialStoreTests {
    @Test("access token is stored, replaced and removed through Keychain")
    func keychainRoundTrip() async throws {
        let service = "com.chatos.tests.authentication.\(UUID().uuidString)"
        let account = "access-token"
        let broker = testKeychainBroker()
        let writer = KeychainCredentialStore(
            service: service,
            account: account,
            broker: broker
        )
        do {
            #expect(try await writer.loadAccessToken() == nil)
            try await writer.saveAccessToken("first-token")
            #expect(try await writer.loadAccessToken() == "first-token")

            let independentReader = KeychainCredentialStore(
                service: service,
                account: account,
                broker: broker
            )
            #expect(try await independentReader.loadAccessToken() == "first-token")
            try await writer.saveAccessToken("second-token")

            let replacedReader = KeychainCredentialStore(
                service: service,
                account: account,
                broker: broker
            )
            #expect(try await replacedReader.loadAccessToken() == "second-token")
            try await writer.deleteAccessToken()

            let deletedReader = KeychainCredentialStore(
                service: service,
                account: account,
                broker: broker
            )
            #expect(try await deletedReader.loadAccessToken() == nil)
        } catch {
            try? await writer.deleteAccessToken()
            throw error
        }
        try? await writer.deleteAccessToken()
    }

    private func testKeychainBroker() -> MacOSKeychainBrokerClient {
        let packageDirectory = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        return MacOSKeychainBrokerClient(
            executableURL: packageDirectory
                .appendingPathComponent(".build", isDirectory: true)
                .appendingPathComponent("arm64-apple-macosx", isDirectory: true)
                .appendingPathComponent("debug", isDirectory: true)
                .appendingPathComponent("ChatOSKeychainBroker", isDirectory: false)
        )
    }
}
