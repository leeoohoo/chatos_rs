import Foundation
import LocalAuthentication
import Security

struct NativeLocalAgentModelCredentialStore: Sendable {
    // v1 entries were created by locally packaged builds whose designated requirement changed
    // between releases. Never query or update that namespace: touching those legacy ACLs can
    // launch SecurityAgent even when the operation requests a non-interactive LAContext.
    static let service = "com.chatos.swift.local-agent-model.v2"

    func loadWithoutUserInteraction(
        ownerUserID: String,
        modelConfigRef: String
    ) throws -> String? {
        let query = Self.loadQuery(
            ownerUserID: ownerUserID,
            modelConfigRef: modelConfigRef,
            allowUserInteraction: false
        )
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound
            || status == errSecInteractionNotAllowed
            || status == errSecAuthFailed
            || status == errSecUserCanceled {
            return nil
        }
        guard status == errSecSuccess,
              let data = result as? Data,
              let value = String(data: data, encoding: .utf8) else {
            throw NativeLocalAgentModelCredentialError.keychain(status)
        }
        return value
    }

    @discardableResult
    func saveWithoutUserInteraction(
        _ credential: String,
        ownerUserID: String,
        modelConfigRef: String
    ) throws -> Bool {
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
        var updateSelector = selector
        updateSelector[kSecUseAuthenticationContext as String] = Self.nonInteractiveContext()
        let data = Data(credential.utf8)
        let update = SecItemUpdate(
            updateSelector as CFDictionary,
            [kSecValueData as String: data] as CFDictionary
        )
        if update == errSecSuccess { return true }
        if update == errSecInteractionNotAllowed
            || update == errSecAuthFailed
            || update == errSecUserCanceled {
            return false
        }
        guard update == errSecItemNotFound else {
            throw NativeLocalAgentModelCredentialError.keychain(update)
        }
        var insert = selector
        insert[kSecValueData as String] = data
        insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(insert as CFDictionary, nil)
        if status == errSecDuplicateItem { return false }
        guard status == errSecSuccess else {
            throw NativeLocalAgentModelCredentialError.keychain(status)
        }
        return true
    }

    static func loadQuery(
        ownerUserID: String,
        modelConfigRef: String,
        allowUserInteraction: Bool
    ) -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account(ownerUserID, modelConfigRef),
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        if !allowUserInteraction {
            query[kSecUseAuthenticationContext as String] = nonInteractiveContext()
        }
        return query
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

    private static func account(_ ownerUserID: String, _ modelConfigRef: String) -> String {
        "v2:\(ownerUserID):\(modelConfigRef)"
    }

    private static func nonInteractiveContext() -> LAContext {
        let context = LAContext()
        context.interactionNotAllowed = true
        return context
    }

    private func account(_ ownerUserID: String, _ modelConfigRef: String) -> String {
        Self.account(ownerUserID, modelConfigRef)
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
