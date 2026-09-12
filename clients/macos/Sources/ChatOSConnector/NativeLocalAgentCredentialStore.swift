// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation
import LocalAuthentication
import Security

public enum NativeLocalAgentCredentialStoreError: Error, Equatable, Sendable {
    case invalidReference
    case keychain(operation: String, status: OSStatus)
}

/// Account-scoped secure storage used by the signed Host bootstrap channel.
/// Local Agent credentials are never mirrored into preferences, SQLite, JSON
/// files, environment variables, or diagnostic payloads. Every operation uses
/// a non-interactive authentication context so a background lifecycle task
/// fails instead of opening a password or biometric prompt.
public actor NativeLocalAgentCredentialStore {
    public static let productionService = "com.chatos.local-agent.credentials.v2"

    private let service: String

    public init(service: String = productionService) throws {
        guard Self.valid(service) else {
            throw NativeLocalAgentCredentialStoreError.invalidReference
        }
        self.service = service
    }

    public func load(accountID: String, reference: String) throws -> Data? {
        let account = try accountKey(accountID: accountID, reference: reference)
        var query = nonInteractiveQuery(account: account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else {
            throw NativeLocalAgentCredentialStoreError.keychain(
                operation: "load",
                status: status
            )
        }
        return data
    }

    public func save(_ secret: Data, accountID: String, reference: String) throws {
        guard !secret.isEmpty else {
            throw NativeLocalAgentCredentialStoreError.invalidReference
        }
        let account = try accountKey(accountID: accountID, reference: reference)
        let query = nonInteractiveQuery(account: account)
        let update: [String: Any] = [kSecValueData as String: secret]
        let updateStatus = SecItemUpdate(query as CFDictionary, update as CFDictionary)
        if updateStatus == errSecSuccess { return }
        guard updateStatus == errSecItemNotFound else {
            throw NativeLocalAgentCredentialStoreError.keychain(
                operation: "update",
                status: updateStatus
            )
        }
        var addition = query
        addition[kSecValueData as String] = secret
        addition[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        addition[kSecAttrSynchronizable as String] = false
        let addStatus = SecItemAdd(addition as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw NativeLocalAgentCredentialStoreError.keychain(
                operation: "add",
                status: addStatus
            )
        }
    }

    public func delete(accountID: String, reference: String) throws {
        let account = try accountKey(accountID: accountID, reference: reference)
        let status = SecItemDelete(nonInteractiveQuery(account: account) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw NativeLocalAgentCredentialStoreError.keychain(
                operation: "delete",
                status: status
            )
        }
    }

    func isUnlocked() -> Bool {
        var keychain: SecKeychain?
        guard SecKeychainCopyDefault(&keychain) == errSecSuccess, let keychain else {
            return false
        }
        var status: SecKeychainStatus = 0
        return SecKeychainGetStatus(keychain, &status) == errSecSuccess
            && status & UInt32(kSecUnlockStateStatus) != 0
    }

    private func nonInteractiveQuery(account: String) -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let context = LAContext()
        context.interactionNotAllowed = true
        query[kSecUseAuthenticationContext as String] = context
        return query
    }

    private func accountKey(accountID: String, reference: String) throws -> String {
        guard Self.valid(accountID), Self.valid(reference) else {
            throw NativeLocalAgentCredentialStoreError.invalidReference
        }
        return "v1:\(accountID.utf8.count):\(accountID)\(reference)"
    }

    private static func valid(_ value: String) -> Bool {
        !value.isEmpty
            && value.count <= 512
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }
}
