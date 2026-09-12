import ChatOSCore
import Foundation
import LocalAuthentication
import Security

actor KeychainCredentialStore: CredentialStoring {
    private let service: String
    private let account: String
    private var cachedAccessToken: String?
    private var hasLoadedAccessToken = false

    init(
        service: String = "com.chatos.swift-client.authentication",
        account: String = "access-token"
    ) {
        precondition(!service.isEmpty && !account.isEmpty)
        self.service = service
        self.account = account
    }

    func loadAccessToken() async throws -> String? {
        if hasLoadedAccessToken { return cachedAccessToken }

        var query = nonInteractiveQuery
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound {
            cachedAccessToken = nil
            hasLoadedAccessToken = true
            return nil
        }
        guard status == errSecSuccess, let data = result as? Data else {
            throw keychainError(status)
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

        let data = Data(normalized.utf8)
        let updateStatus = SecItemUpdate(
            nonInteractiveQuery as CFDictionary,
            [kSecValueData as String: data] as CFDictionary
        )
        if updateStatus == errSecItemNotFound {
            var addition = nonInteractiveQuery
            addition[kSecValueData as String] = data
            addition[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            addition[kSecAttrSynchronizable as String] = false
            let addStatus = SecItemAdd(addition as CFDictionary, nil)
            guard addStatus == errSecSuccess else { throw keychainError(addStatus) }
        } else if updateStatus != errSecSuccess {
            throw keychainError(updateStatus)
        }
        cachedAccessToken = normalized
        hasLoadedAccessToken = true
    }

    func deleteAccessToken() async throws {
        if hasLoadedAccessToken, cachedAccessToken == nil { return }
        let status = SecItemDelete(nonInteractiveQuery as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw keychainError(status)
        }
        cachedAccessToken = nil
        hasLoadedAccessToken = true
    }

    private var baseQuery: [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrSynchronizable as String: false,
        ]
    }

    private var nonInteractiveQuery: [String: Any] {
        let context = LAContext()
        context.interactionNotAllowed = true
        var query = baseQuery
        query[kSecUseAuthenticationContext as String] = context
        return query
    }

    private func keychainError(_ status: OSStatus) -> NSError {
        NSError(
            domain: NSOSStatusErrorDomain,
            code: Int(status),
            userInfo: [NSLocalizedDescriptionKey: "macOS Keychain access failed (\(status))"]
        )
    }
}
