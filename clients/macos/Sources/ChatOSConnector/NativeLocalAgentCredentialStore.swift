// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation
import LocalAuthentication
import Security

public enum NativeLocalAgentCredentialStoreError: Error, Equatable, Sendable {
    case invalidReference
    case keychain(OSStatus)
}

/// Account-scoped secure storage shared by the native bootstrap builder and
/// the Rust Host's macOS Keychain adapter. Local Agent credentials are never
/// mirrored into preferences, SQLite, JSON files, or diagnostic payloads.
public actor NativeLocalAgentCredentialStore {
    public static let productionService = "com.chatos.local-agent.credentials.v1"

    private let service: String

    public init(service: String = productionService) throws {
        guard Self.valid(service) else {
            throw NativeLocalAgentCredentialStoreError.invalidReference
        }
        self.service = service
    }

    public func load(accountID: String, reference: String) throws -> Data? {
        let account = try accountKey(accountID: accountID, reference: reference)
        let context = LAContext()
        context.interactionNotAllowed = true
        var query = baseQuery(account: account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        query[kSecUseAuthenticationContext as String] = context
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else {
            throw NativeLocalAgentCredentialStoreError.keychain(status)
        }
        return data
    }

    public func save(_ secret: Data, accountID: String, reference: String) throws {
        guard !secret.isEmpty else {
            throw NativeLocalAgentCredentialStoreError.invalidReference
        }
        let account = try accountKey(accountID: accountID, reference: reference)
        let query = baseQuery(account: account)
        let update: [String: Any] = [kSecValueData as String: secret]
        let updateStatus = SecItemUpdate(query as CFDictionary, update as CFDictionary)
        if updateStatus == errSecSuccess { return }
        guard updateStatus == errSecItemNotFound else {
            throw NativeLocalAgentCredentialStoreError.keychain(updateStatus)
        }
        var addition = query
        addition[kSecValueData as String] = secret
        addition[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        addition[kSecAttrSynchronizable as String] = false
        let addStatus = SecItemAdd(addition as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw NativeLocalAgentCredentialStoreError.keychain(addStatus)
        }
    }

    public func delete(accountID: String, reference: String) throws {
        let account = try accountKey(accountID: accountID, reference: reference)
        let status = SecItemDelete(baseQuery(account: account) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw NativeLocalAgentCredentialStoreError.keychain(status)
        }
    }

    private func baseQuery(account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    private func accountKey(accountID: String, reference: String) throws -> String {
        guard Self.valid(accountID), Self.valid(reference) else {
            throw NativeLocalAgentCredentialStoreError.invalidReference
        }
        return "\(accountID)/\(reference)"
    }

    private static func valid(_ value: String) -> Bool {
        !value.isEmpty
            && value.count <= 512
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }
}
