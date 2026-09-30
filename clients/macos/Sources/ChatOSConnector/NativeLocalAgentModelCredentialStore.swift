import Foundation
import Security

struct NativeLocalAgentModelCredentialStore: Sendable {
    private static let service = "com.chatos.swift.local-agent-model"

    func load(ownerUserID: String, modelConfigRef: String) throws -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account(ownerUserID, modelConfigRef),
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess,
              let data = result as? Data,
              let value = String(data: data, encoding: .utf8) else {
            throw NativeLocalAgentModelCredentialError.keychain(status)
        }
        return value
    }

    func save(_ credential: String, ownerUserID: String, modelConfigRef: String) throws {
        guard !credential.isEmpty,
              credential.lengthOfBytes(using: .utf8) <= 64 * 1_024,
              !credential.contains("\0") else {
            throw NativeLocalAgentModelCredentialError.invalidCredential
        }
        let account = account(ownerUserID, modelConfigRef)
        let selector: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account,
        ]
        let data = Data(credential.utf8)
        let update = SecItemUpdate(
            selector as CFDictionary,
            [kSecValueData as String: data] as CFDictionary
        )
        if update == errSecSuccess { return }
        guard update == errSecItemNotFound else {
            throw NativeLocalAgentModelCredentialError.keychain(update)
        }
        var insert = selector
        insert[kSecValueData as String] = data
        insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(insert as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw NativeLocalAgentModelCredentialError.keychain(status)
        }
    }

    func delete(ownerUserID: String, modelConfigRef: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account(ownerUserID, modelConfigRef),
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw NativeLocalAgentModelCredentialError.keychain(status)
        }
    }

    func environmentVariable(modelConfigRef: String) -> String {
        let normalized = modelConfigRef.uppercased().unicodeScalars.map { scalar in
            CharacterSet.uppercaseLetters.contains(scalar)
                || CharacterSet.decimalDigits.contains(scalar) ? String(scalar) : "_"
        }.joined()
        return "CHATOS_LOCAL_AGENT_MODEL_\(normalized.prefix(96))"
    }

    private func account(_ ownerUserID: String, _ modelConfigRef: String) -> String {
        "v1:\(ownerUserID):\(modelConfigRef)"
    }
}

enum NativeLocalAgentModelCredentialError: LocalizedError {
    case invalidCredential
    case keychain(OSStatus)

    var errorDescription: String? {
        switch self {
        case .invalidCredential:
            "Local Agent model credential is invalid."
        case let .keychain(status):
            "Local Agent model credential Keychain operation failed (\(status))."
        }
    }
}
