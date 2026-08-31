import ChatOSCore
import Foundation
import LocalAuthentication
import Security

actor KeychainCredentialStore: CredentialStoring {
    private let credentialURL: URL
    private var cachedAccessToken: String?
    private var hasLoadedAccessToken = false

    init(
        service: String = "com.chatos.swift-client.authentication",
        account: String = "access-token"
    ) {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser
        self.credentialURL = support
            .appendingPathComponent("ChatOSSwift", isDirectory: true)
            .appendingPathComponent("Credentials", isDirectory: true)
            .appendingPathComponent(account, isDirectory: false)
    }

    func loadAccessToken() async throws -> String? {
        if hasLoadedAccessToken { return cachedAccessToken }

        if !FileManager.default.fileExists(atPath: credentialURL.path),
           let legacy = loadLegacyKeychainWithoutUI() {
            try persist(legacy)
        }
        guard FileManager.default.fileExists(atPath: credentialURL.path) else {
            cachedAccessToken = nil
            hasLoadedAccessToken = true
            return nil
        }
        let data = try Data(contentsOf: credentialURL)

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
        try FileManager.default.createDirectory(
            at: credentialURL.deletingLastPathComponent(),
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try data.write(to: credentialURL, options: [.atomic])
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: credentialURL.path)
    }

    func deleteAccessToken() async throws {
        if hasLoadedAccessToken, cachedAccessToken == nil { return }
        if FileManager.default.fileExists(atPath: credentialURL.path) {
            try FileManager.default.removeItem(at: credentialURL)
        }
        cachedAccessToken = nil
        hasLoadedAccessToken = true
    }

    private func loadLegacyKeychainWithoutUI() -> Data? {
        let context = LAContext()
        context.interactionNotAllowed = true
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "com.chatos.swift-client.authentication",
            kSecAttrAccount as String: "access-token",
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
            kSecUseAuthenticationContext as String: context,
        ]
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess else { return nil }
        return result as? Data
    }
}
