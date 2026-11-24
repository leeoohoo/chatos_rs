import Foundation
import LocalAuthentication
import Security

public enum KeychainLegacyItemPolicy: Sendable {
    case all
    case none
    case accountPrefix(String)

    fileprivate func allows(_ account: String) -> Bool {
        switch self {
        case .all: true
        case .none: false
        case let .accountPrefix(prefix): account.hasPrefix(prefix)
        }
    }
}

private struct KeychainVault: Codable, Sendable {
    static let currentVersion = 1

    var version = currentVersion
    var entries: [String: Data] = [:]
}

private final class KeychainVaultCache: @unchecked Sendable {
    let lock = NSLock()
    var vaults: [String: KeychainVault] = [:]

    func withLock<T>(_ operation: () throws -> T) rethrows -> T {
        lock.lock()
        defer { lock.unlock() }
        return try operation()
    }
}

public struct KeychainSecretStore: Sendable {
    public static let chatOSVaultService = "com.chatos.swift-client.secure-vault"

    private static let vaultAccount = "credential-vault-v1"
    private static let maximumEntryBytes = 64 * 1_024
    private static let maximumVaultBytes = 2 * 1_024 * 1_024
    private static let maximumVaultEntries = 4_096
    private static let cache = KeychainVaultCache()

    private let service: String
    private let vaultService: String
    private let legacyItemPolicy: KeychainLegacyItemPolicy

    public init(
        service: String,
        vaultService: String? = nil,
        legacyItemPolicy: KeychainLegacyItemPolicy = .all
    ) {
        self.service = service
        self.vaultService = vaultService ?? service
        self.legacyItemPolicy = legacyItemPolicy
    }

    public func load(account: String, migrateLegacy: Bool = true) throws -> Data? {
        try Self.cache.withLock {
            var vault = try loadVaultLocked()
            let key = try vaultKey(account: account)
            if let stored = vault.entries[key] { return stored }
            guard migrateLegacy,
                  legacyItemPolicy.allows(account),
                  let legacy = try loadLegacy(account: account) else { return nil }
            guard legacy.count <= Self.maximumEntryBytes else {
                throw KeychainSecretStoreError(status: errSecDataTooLarge)
            }
            vault.entries[key] = legacy
            try saveVaultLocked(vault)
            return legacy
        }
    }

    public func save(_ data: Data, account: String) throws {
        guard data.count <= Self.maximumEntryBytes else {
            throw KeychainSecretStoreError(status: errSecDataTooLarge)
        }
        try Self.cache.withLock {
            var vault = try loadVaultLocked()
            vault.entries[try vaultKey(account: account)] = data
            try saveVaultLocked(vault)
        }
    }

    /// Restores a pre-vault item even when a replacement was already written to the vault.
    public func restoreLegacyItem(account: String) throws -> Data? {
        try Self.cache.withLock {
            guard legacyItemPolicy.allows(account),
                  let legacy = try loadLegacy(account: account) else { return nil }
            guard legacy.count <= Self.maximumEntryBytes else {
                throw KeychainSecretStoreError(status: errSecDataTooLarge)
            }
            var vault = try loadVaultLocked()
            vault.entries[try vaultKey(account: account)] = legacy
            try saveVaultLocked(vault)
            let status = SecItemDelete(legacyQuery(account: account) as CFDictionary)
            guard status == errSecSuccess || status == errSecItemNotFound else {
                throw KeychainSecretStoreError(status: status)
            }
            return legacy
        }
    }

    public func delete(account: String) throws {
        try Self.cache.withLock {
            var vault = try loadVaultLocked()
            if vault.entries.removeValue(forKey: try vaultKey(account: account)) != nil {
                try saveVaultLocked(vault)
            }
            guard legacyItemPolicy.allows(account) else { return }
            let status = SecItemDelete(legacyQuery(account: account) as CFDictionary)
            guard status == errSecSuccess || status == errSecItemNotFound else {
                throw KeychainSecretStoreError(status: status)
            }
        }
    }

    private func loadVaultLocked() throws -> KeychainVault {
        if let cached = Self.cache.vaults[vaultService] { return cached }
        let context = LAContext()
        context.interactionNotAllowed = true
        var result: CFTypeRef?
        let status = SecItemCopyMatching([
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: vaultService,
            kSecAttrAccount as String: Self.vaultAccount,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
            kSecUseAuthenticationContext as String: context,
        ] as CFDictionary, &result)
        if status == errSecItemNotFound {
            let vault = KeychainVault()
            Self.cache.vaults[vaultService] = vault
            return vault
        }
        guard status == errSecSuccess,
              let data = result as? Data,
              data.count <= Self.maximumVaultBytes,
              let vault = try? JSONDecoder().decode(KeychainVault.self, from: data),
              vault.version == KeychainVault.currentVersion,
              vault.entries.count <= Self.maximumVaultEntries,
              vault.entries.values.allSatisfy({ $0.count <= Self.maximumEntryBytes }) else {
            throw KeychainSecretStoreError(status: status == errSecSuccess ? errSecDecode : status)
        }
        Self.cache.vaults[vaultService] = vault
        return vault
    }

    private func saveVaultLocked(_ vault: KeychainVault) throws {
        guard vault.entries.count <= Self.maximumVaultEntries,
              vault.entries.values.allSatisfy({ $0.count <= Self.maximumEntryBytes }),
              let data = try? JSONEncoder().encode(vault),
              data.count <= Self.maximumVaultBytes else {
            throw KeychainSecretStoreError(status: errSecDataTooLarge)
        }
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: vaultService,
            kSecAttrAccount as String: Self.vaultAccount,
        ]
        let context = LAContext()
        context.interactionNotAllowed = true
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        var status = SecItemUpdate(
            query.merging([kSecUseAuthenticationContext as String: context]) { _, value in value }
                as CFDictionary,
            attributes as CFDictionary
        )
        if status == errSecItemNotFound {
            status = SecItemAdd(
                query.merging(attributes) { _, value in value } as CFDictionary,
                nil
            )
        }
        guard status == errSecSuccess else {
            throw KeychainSecretStoreError(status: status)
        }
        Self.cache.vaults[vaultService] = vault
    }

    private func loadLegacy(account: String) throws -> Data? {
        let context = LAContext()
        context.interactionNotAllowed = true
        var result: CFTypeRef?
        let status = SecItemCopyMatching(
            legacyQuery(account: account).merging([
                kSecReturnData as String: true,
                kSecMatchLimit as String: kSecMatchLimitOne,
                kSecUseAuthenticationContext as String: context,
            ]) { _, value in value } as CFDictionary,
            &result
        )
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else {
            throw KeychainSecretStoreError(status: status)
        }
        return data
    }

    private func legacyQuery(account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    private func vaultKey(account: String) throws -> String {
        let key = "\(service)\u{0}\(account)"
        guard key.lengthOfBytes(using: .utf8) <= 2_048 else {
            throw KeychainSecretStoreError(status: errSecParam)
        }
        return key
    }
}

public struct KeychainSecretStoreError: LocalizedError, Sendable {
    public let status: OSStatus

    public var errorDescription: String? {
        SecCopyErrorMessageString(status, nil) as String?
            ?? "Keychain operation failed (\(status))."
    }
}
