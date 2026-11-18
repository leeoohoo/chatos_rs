import ChatOSCore
import CryptoKit
import Foundation

struct NativeConnectorPersistentState: Codable, Sendable {
    var deploymentIdentifier: String?
    var gatewayBaseURL: String?
    var user: LocalConnectorUser?
    var deviceID: String?
    var deviceName: String?
    var workspaces: [LocalConnectorWorkspace] = []
    /// `false` means the user explicitly blocked server-to-client calls. Optional keeps
    /// existing installations backward compatible: a missing value means enabled.
    var gatewayConnectionEnabled: Bool?
    var developerMode = false
    var approvalMode: LocalConnectorApprovalMode = .requestApproval
    var commandApprovalModelConfigID: String?
    var commandApprovalThinkingLevel: String?
    var sandboxEnabled = true
    var permissionProfileID = ":workspace-write"
    var approvalPolicy = "per_call"
    var approvalReviewer = "user"
    var networkAccess = "restricted"
    var policyRevision: String?
    var installedPluginIDs: Set<String> = []
    var installedPluginRecords: [String: NativeInstalledPluginRecord]?
    var pluginHookTrustRecords: [String: NativePluginHookTrustRecord]?
    var pluginPreferences: [String: Bool] = [:]
    /// Associates each locally cached model with the managed Prompt/capability revision
    /// selected for its vendor. The actual prompt and tools remain in Local Agent Host's
    /// durable capability store; this map only lets wake/offline restore select the right one.
    var localAgentCapabilityRevisionsByModelConfigID: [String: String]?
    var localAgentPromptBundleVersion: Int64?
    var localAgentTaskSkillChecksum: String?

    static let empty = NativeConnectorPersistentState()
}

struct NativeConnectorActivityHistory: Codable, Sendable, Equatable {
    var approvals: [LocalConnectorApprovalHistoryEntry] = []
    var commands: [LocalConnectorCommandHistoryEntry] = []

    static let empty = NativeConnectorActivityHistory()
}

struct NativeConnectorActivityHistoryStore: Sendable {
    static let maximumHistoryBytes = 16 * 1_024 * 1_024
    let historyURL: URL

    func load() throws -> NativeConnectorActivityHistory {
        guard FileManager.default.fileExists(atPath: historyURL.path) else { return .empty }
        return try JSONDecoder().decode(
            NativeConnectorActivityHistory.self,
            from: NativeBoundedFileReader.read(
                historyURL,
                maximumBytes: Self.maximumHistoryBytes
            )
        )
    }

    func save(_ history: NativeConnectorActivityHistory) throws {
        try FileManager.default.createDirectory(
            at: historyURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(history)
        guard data.count <= Self.maximumHistoryBytes else {
            throw NativeBoundedFileReadError.fileTooLarge(
                maximumBytes: Self.maximumHistoryBytes
            )
        }
        try data.write(to: historyURL, options: .atomic)
    }
}

struct NativeInstalledPluginRecord: Codable, Sendable, Equatable {
    var pluginID: String
    var releaseID: String
    var version: String
    var artifactSHA256: String
    var installationPath: String
    var installedAt: String
    var pluginKey: String? = nil
    var packageFileSHA256: [String: String]? = nil
    var source: LocalConnectorPluginSource? = nil
    var sourceSummary: String? = nil
    var sourceDraft: LocalConnectorPluginSourceDraft? = nil
    var customMCPDraft: LocalConnectorCustomMCPDraft? = nil
}

struct NativeConnectorStateStore: Sendable {
    static let maximumStateBytes = 16 * 1_024 * 1_024
    let stateURL: URL

    func load() throws -> NativeConnectorPersistentState {
        guard FileManager.default.fileExists(atPath: stateURL.path) else { return .empty }
        return try JSONDecoder().decode(
            NativeConnectorPersistentState.self,
            from: NativeBoundedFileReader.read(
                stateURL,
                maximumBytes: Self.maximumStateBytes
            )
        )
    }

    func save(_ state: NativeConnectorPersistentState) throws {
        try FileManager.default.createDirectory(
            at: stateURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(state)
        guard data.count <= Self.maximumStateBytes else {
            throw NativeBoundedFileReadError.fileTooLarge(
                maximumBytes: Self.maximumStateBytes
            )
        }
        try data.write(to: stateURL, options: .atomic)
    }
}

struct NativeConnectorSecretStore: Sendable {
    private static let maximumSecretBytes = 64 * 1_024
    private let rootURL: URL
    private let keychain: KeychainSecretStore
    private let legacyKeychain: KeychainSecretStore?

    init(
        rootURL: URL? = nil,
        migrateLegacyKeychain: Bool? = nil,
        legacyKeychainService: String = "com.chatos.swift.native-connector"
    ) {
        let resolvedRootURL: URL
        if let rootURL {
            resolvedRootURL = rootURL
        } else {
            let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
                ?? FileManager.default.homeDirectoryForCurrentUser
            resolvedRootURL = support
                .appendingPathComponent("ChatOSSwift", isDirectory: true)
                .appendingPathComponent("NativeConnector", isDirectory: true)
                .appendingPathComponent("Secrets", isDirectory: true)
        }
        self.rootURL = resolvedRootURL
        self.keychain = KeychainSecretStore(service: Self.keychainService(rootURL: resolvedRootURL))
        self.legacyKeychain = (migrateLegacyKeychain ?? (rootURL == nil))
            ? KeychainSecretStore(service: legacyKeychainService)
            : nil
    }

    func load(account: String) throws -> Data? {
        let url = secretURL(account: account)
        if let stored = try keychain.load(account: account) {
            try legacyKeychain?.delete(account: account)
            try deleteLegacyFile(url)
            return stored
        }
        if let legacyKeychain, let stored = try legacyKeychain.load(account: account) {
            try keychain.save(stored, account: account)
            try legacyKeychain.delete(account: account)
            try deleteLegacyFile(url)
            return stored
        }
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        let stored = try NativeBoundedFileReader.read(url, maximumBytes: Self.maximumSecretBytes)
        try keychain.save(stored, account: account)
        try deleteLegacyFile(url)
        return stored
    }

    func save(_ value: Data, account: String) throws {
        guard value.count <= Self.maximumSecretBytes else {
            throw NativeBoundedFileReadError.fileTooLarge(
                maximumBytes: Self.maximumSecretBytes
            )
        }
        let url = secretURL(account: account)
        try keychain.save(value, account: account)
        try legacyKeychain?.delete(account: account)
        try deleteLegacyFile(url)
    }

    func delete(account: String) throws {
        let url = secretURL(account: account)
        try deleteLegacyFile(url)
        try legacyKeychain?.delete(account: account)
        try keychain.delete(account: account)
    }

    private func secretURL(account: String) -> URL {
        let safeName = account.unicodeScalars.map { scalar in
            CharacterSet.alphanumerics.contains(scalar) || scalar == "-" ? String(scalar) : "_"
        }.joined()
        return rootURL.appendingPathComponent(safeName, isDirectory: false)
    }

    private func deleteLegacyFile(_ url: URL) throws {
        if FileManager.default.fileExists(atPath: url.path) {
            try FileManager.default.removeItem(at: url)
        }
    }

    private static func keychainService(rootURL: URL) -> String {
        let digest = SHA256.hash(data: Data(rootURL.standardizedFileURL.path.utf8))
            .prefix(16)
            .map { String(format: "%02x", $0) }
            .joined()
        return "com.chatos.swift.native-connector.\(digest)"
    }
}

struct NativeConnectorDeviceIdentity: Sendable {
    private static let account = "device-signing-key-v1"
    private let privateKey: Curve25519.Signing.PrivateKey

    init(secretStore: NativeConnectorSecretStore) throws {
        if let stored = try secretStore.load(account: Self.account) {
            do {
                privateKey = try Curve25519.Signing.PrivateKey(rawRepresentation: stored)
                return
            } catch {
                try secretStore.delete(account: Self.account)
            }
        }
        let generated = Curve25519.Signing.PrivateKey()
        try secretStore.save(generated.rawRepresentation, account: Self.account)
        privateKey = generated
    }

    init(privateKey: Curve25519.Signing.PrivateKey) {
        self.privateKey = privateKey
    }

    var publicKey: String {
        "ed25519:\(privateKey.publicKey.rawRepresentation.base64URLEncodedString())"
    }

    func signature(for payload: Data) throws -> String {
        try privateKey.signature(for: payload).base64URLEncodedString()
    }
}

enum NativeConnectorDeviceAuthentication {
    static func connectionPayload(
        deviceID: String,
        timestamp: String,
        nonce: String,
        path: String
    ) -> Data {
        Data("v1\n\(deviceID)\n\(timestamp)\n\(nonce)\n\(path)".utf8)
    }
}

extension Data {
    func base64URLEncodedString() -> String {
        base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}
