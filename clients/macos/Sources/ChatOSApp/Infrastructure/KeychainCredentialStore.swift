import ChatOSCore
import ChatOSConnector
import Foundation
import Security

actor KeychainCredentialStore: CredentialStoring {
    private let service: String
    private let account: String
    private var cachedAccessToken: String?
    private var hasLoadedAccessToken = false
    private let broker: MacOSKeychainBrokerClient

    init(
        service: String = "com.chatos.swift-client.authentication.v6",
        account: String = "access-token-v2",
        broker: MacOSKeychainBrokerClient = .init()
    ) {
        precondition(!service.isEmpty && !account.isEmpty)
        self.service = service
        self.account = account
        self.broker = broker
    }

    func loadAccessToken() async throws -> String? {
        if hasLoadedAccessToken { return cachedAccessToken }

        let data: Data?
        do {
            data = try broker.load(service: service, account: account)
        } catch {
            throw brokerError(error)
        }
        guard let data else {
            cachedAccessToken = nil
            hasLoadedAccessToken = true
            return nil
        }

        let token = String(decoding: data, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        cachedAccessToken = token.isEmpty ? nil : token
        hasLoadedAccessToken = true
        return cachedAccessToken
    }

    func saveAccessToken(_ token: String) async throws {
        let normalized = token.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else {
            try await deleteAccessToken()
            return
        }
        if hasLoadedAccessToken, cachedAccessToken == normalized { return }

        do {
            try broker.save(Data(normalized.utf8), service: service, account: account)
        } catch {
            throw brokerError(error)
        }
        cachedAccessToken = normalized
        hasLoadedAccessToken = true
    }

    func deleteAccessToken() async throws {
        if hasLoadedAccessToken, cachedAccessToken == nil { return }
        do {
            try broker.delete(service: service, account: account)
        } catch {
            throw brokerError(error)
        }
        cachedAccessToken = nil
        hasLoadedAccessToken = true
    }

    private func keychainError(_ status: OSStatus) -> NSError {
        NSError(
            domain: NSOSStatusErrorDomain,
            code: Int(status),
            userInfo: [NSLocalizedDescriptionKey: "macOS Keychain access failed (\(status))"]
        )
    }

    private func brokerError(_ error: Error) -> NSError {
        if case let MacOSKeychainBrokerError.status(status) = error {
            return keychainError(status)
        }
        return keychainError(errSecNotAvailable)
    }
}
