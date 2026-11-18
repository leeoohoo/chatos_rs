import ChatOSCore
import Foundation

actor KeychainCredentialStore: CredentialStoring {
    private static let maximumCredentialBytes = 64 * 1_024
    private let keychain: KeychainSecretStore
    private let account: String
    private let credentialURL: URL
    private var cachedAccessToken: String?
    private var hasLoadedAccessToken = false

    init(
        service: String = "com.chatos.swift-client.authentication",
        account: String = "access-token",
        legacyCredentialURL: URL? = nil
    ) {
        self.keychain = KeychainSecretStore(service: service)
        self.account = account
        if let legacyCredentialURL {
            self.credentialURL = legacyCredentialURL
            return
        }
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser
        self.credentialURL = support
            .appendingPathComponent("ChatOSSwift", isDirectory: true)
            .appendingPathComponent("Credentials", isDirectory: true)
            .appendingPathComponent(account, isDirectory: false)
    }

    func loadAccessToken() async throws -> String? {
        if hasLoadedAccessToken { return cachedAccessToken }
        if ProcessInfo.processInfo.environment["CHATOS_BUILD_SMOKE_TEST"] == "1" {
            hasLoadedAccessToken = true
            return nil
        }
        let data: Data
        if let stored = try keychain.load(account: account) {
            data = stored
            try deleteLegacyFile()
        } else if FileManager.default.fileExists(atPath: credentialURL.path) {
            data = try AppBoundedFileReader.read(
                credentialURL,
                maximumBytes: Self.maximumCredentialBytes
            )
            try keychain.save(data, account: account)
            try deleteLegacyFile()
        } else {
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

        try persist(Data(normalized.utf8))
        cachedAccessToken = normalized
        hasLoadedAccessToken = true
    }

    private func persist(_ data: Data) throws {
        guard data.count <= Self.maximumCredentialBytes else {
            throw AppBoundedFileReadError.fileTooLarge(
                maximumBytes: Self.maximumCredentialBytes
            )
        }
        try keychain.save(data, account: account)
        try deleteLegacyFile()
    }

    func deleteAccessToken() async throws {
        try deleteLegacyFile()
        try keychain.delete(account: account)
        cachedAccessToken = nil
        hasLoadedAccessToken = true
    }

    private func deleteLegacyFile() throws {
        if FileManager.default.fileExists(atPath: credentialURL.path) {
            try FileManager.default.removeItem(at: credentialURL)
        }
    }
}
