import ChatOSCore
import CryptoKit
import Foundation

struct NativeLocalAgentModelCredentialStore: Sendable {
    static let maximumCredentialBytes = 64 * 1_024
    private let rootURL: URL
    private let keychain: KeychainSecretStore

    init(rootURL: URL? = nil) {
        let resolvedRootURL: URL
        if let rootURL {
            resolvedRootURL = rootURL
        } else {
            let support = FileManager.default.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            ).first ?? FileManager.default.homeDirectoryForCurrentUser
            resolvedRootURL = support
                .appendingPathComponent("ChatOSSwift", isDirectory: true)
                .appendingPathComponent("NativeConnector", isDirectory: true)
                .appendingPathComponent("Secrets", isDirectory: true)
                .appendingPathComponent("LocalAgentModels", isDirectory: true)
        }
        self.rootURL = resolvedRootURL
        self.keychain = KeychainSecretStore(service: Self.keychainService(rootURL: resolvedRootURL))
    }

    func loadWithoutUserInteraction(
        ownerUserID: String,
        modelConfigRef: String
    ) throws -> String? {
        let url = credentialURL(ownerUserID: ownerUserID, modelConfigRef: modelConfigRef)
        let account = url.lastPathComponent
        let data: Data
        if let stored = try keychain.load(account: account) {
            data = stored
            try deleteLegacyFile(url)
        } else if FileManager.default.fileExists(atPath: url.path) {
            data = try NativeBoundedFileReader.read(url, maximumBytes: Self.maximumCredentialBytes)
            try keychain.save(data, account: account)
            try deleteLegacyFile(url)
        } else {
            return nil
        }
        guard let value = String(data: data, encoding: .utf8) else {
            throw NativeLocalAgentModelCredentialError.invalidCredential
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
              credential.lengthOfBytes(using: .utf8) <= Self.maximumCredentialBytes,
              !credential.contains("\0") else {
            throw NativeLocalAgentModelCredentialError.invalidCredential
        }
        let url = credentialURL(ownerUserID: ownerUserID, modelConfigRef: modelConfigRef)
        try keychain.save(Data(credential.utf8), account: url.lastPathComponent)
        try deleteLegacyFile(url)
        return true
    }

    func delete(ownerUserID: String, modelConfigRef: String) throws {
        let url = credentialURL(ownerUserID: ownerUserID, modelConfigRef: modelConfigRef)
        try deleteLegacyFile(url)
        try keychain.delete(account: url.lastPathComponent)
    }

    func environmentVariable(modelConfigRef: String) -> String {
        let normalized = modelConfigRef.uppercased().unicodeScalars.map { scalar in
            CharacterSet.uppercaseLetters.contains(scalar)
                || CharacterSet.decimalDigits.contains(scalar) ? String(scalar) : "_"
        }.joined()
        return "CHATOS_LOCAL_AGENT_MODEL_\(normalized.prefix(96))"
    }

    func credentialURL(ownerUserID: String, modelConfigRef: String) -> URL {
        let identity = Data("\(ownerUserID)\u{0}\(modelConfigRef)".utf8)
        let digest = SHA256.hash(data: identity)
            .map { String(format: "%02x", $0) }
            .joined()
        return rootURL.appendingPathComponent("model-\(digest)", isDirectory: false)
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
        return "com.chatos.swift.native-connector.local-agent-models.\(digest)"
    }
}

enum NativeLocalAgentModelCredentialError: LocalizedError {
    case invalidCredential

    var errorDescription: String? {
        switch self {
        case .invalidCredential:
            "Model credential is invalid."
        }
    }
}
