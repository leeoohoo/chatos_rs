import ChatOSCore
import Foundation

struct NativeRemoteConnectionCredentials: Codable, Sendable, Equatable {
    var password: String?
    var privateKeyPath: String?
    var certificatePath: String?
    var jumpPrivateKeyPath: String?
    var jumpCertificatePath: String?
    var jumpPassword: String?

    var isEmpty: Bool {
        [
            password,
            privateKeyPath,
            certificatePath,
            jumpPrivateKeyPath,
            jumpCertificatePath,
            jumpPassword,
        ].allSatisfy { $0?.trimmedNonEmpty == nil }
    }

    func applying(to draft: RemoteConnectionDraft) -> RemoteConnectionDraft {
        var result = draft
        result.password = draft.password?.trimmedNonEmpty ?? password?.trimmedNonEmpty
        result.privateKeyPath = draft.privateKeyPath?.trimmedNonEmpty ?? privateKeyPath?.trimmedNonEmpty
        result.certificatePath = draft.certificatePath?.trimmedNonEmpty ?? certificatePath?.trimmedNonEmpty
        result.jumpPrivateKeyPath = draft.jumpPrivateKeyPath?.trimmedNonEmpty
            ?? jumpPrivateKeyPath?.trimmedNonEmpty
        result.jumpCertificatePath = draft.jumpCertificatePath?.trimmedNonEmpty
            ?? jumpCertificatePath?.trimmedNonEmpty
        result.jumpPassword = draft.jumpPassword?.trimmedNonEmpty ?? jumpPassword?.trimmedNonEmpty
        return result
    }

    static func from(_ draft: RemoteConnectionDraft) -> NativeRemoteConnectionCredentials {
        NativeRemoteConnectionCredentials(
            password: draft.authenticationType == .password
                ? draft.password?.trimmedNonEmpty
                : nil,
            privateKeyPath: draft.authenticationType == .password
                ? nil
                : draft.privateKeyPath?.trimmedNonEmpty,
            certificatePath: draft.authenticationType == .privateKeyCertificate
                ? draft.certificatePath?.trimmedNonEmpty
                : nil,
            jumpPrivateKeyPath: draft.jumpPrivateKeyPath?.trimmedNonEmpty,
            jumpCertificatePath: draft.jumpCertificatePath?.trimmedNonEmpty,
            jumpPassword: draft.jumpPassword?.trimmedNonEmpty
        )
    }
}

struct NativeRemoteConnectionCredentialStore: Sendable {
    private let secretStore: NativeConnectorSecretStore

    init(secretStore: NativeConnectorSecretStore = NativeConnectorSecretStore()) {
        self.secretStore = secretStore
    }

    func load(connectionID: String) throws -> NativeRemoteConnectionCredentials? {
        guard let data = try secretStore.load(account: account(connectionID)) else { return nil }
        return try JSONDecoder().decode(NativeRemoteConnectionCredentials.self, from: data)
    }

    func save(_ credentials: NativeRemoteConnectionCredentials, connectionID: String) throws {
        if credentials.isEmpty {
            try delete(connectionID: connectionID)
            return
        }
        try secretStore.save(
            JSONEncoder().encode(credentials),
            account: account(connectionID)
        )
    }

    func delete(connectionID: String) throws {
        try secretStore.delete(account: account(connectionID))
    }

    private func account(_ connectionID: String) -> String {
        "remote-connection-credentials-v1:\(connectionID)"
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
