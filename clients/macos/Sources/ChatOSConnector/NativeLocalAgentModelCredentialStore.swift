import CryptoKit
import Foundation

struct NativeLocalAgentModelCredentialStore: Sendable {
    static let maximumCredentialBytes = 64 * 1_024
    private let rootURL: URL

    init(rootURL: URL? = nil) {
        if let rootURL {
            self.rootURL = rootURL
            return
        }
        let support = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first ?? FileManager.default.homeDirectoryForCurrentUser
        self.rootURL = support
            .appendingPathComponent("ChatOSSwift", isDirectory: true)
            .appendingPathComponent("NativeConnector", isDirectory: true)
            .appendingPathComponent("Secrets", isDirectory: true)
            .appendingPathComponent("LocalAgentModels", isDirectory: true)
    }

    func loadWithoutUserInteraction(
        ownerUserID: String,
        modelConfigRef: String
    ) throws -> String? {
        let url = credentialURL(ownerUserID: ownerUserID, modelConfigRef: modelConfigRef)
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        let data = try NativeBoundedFileReader.read(
            url,
            maximumBytes: Self.maximumCredentialBytes
        )
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
        try FileManager.default.createDirectory(
            at: rootURL,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: rootURL.path
        )
        let url = credentialURL(ownerUserID: ownerUserID, modelConfigRef: modelConfigRef)
        try Data(credential.utf8).write(to: url, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: url.path
        )
        return true
    }

    func delete(ownerUserID: String, modelConfigRef: String) throws {
        let url = credentialURL(ownerUserID: ownerUserID, modelConfigRef: modelConfigRef)
        guard FileManager.default.fileExists(atPath: url.path) else { return }
        try FileManager.default.removeItem(at: url)
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
}

enum NativeLocalAgentModelCredentialError: LocalizedError {
    case invalidCredential

    var errorDescription: String? {
        switch self {
        case .invalidCredential:
            "Local Agent model credential is invalid."
        }
    }
}
