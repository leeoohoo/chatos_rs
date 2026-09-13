import ChatOSCore
import CryptoKit
import Foundation

struct NativeConnectorPersistentState: Codable, Sendable {
    var user: LocalConnectorUser?
    var deviceID: String?
    var deviceName: String?
    var workspaces: [LocalConnectorWorkspace] = []
    /// `false` means the user explicitly blocked server-to-client calls.
    var gatewayConnectionEnabled: Bool?
    var installedPluginIDs: Set<String> = []
    var installedPluginRecords: [String: NativeInstalledPluginRecord]?
    var pluginPreferences: [String: Bool] = [:]

    static let empty = NativeConnectorPersistentState()
}

struct NativeInstalledPluginRecord: Codable, Sendable, Equatable {
    var pluginID: String
    var releaseID: String
    var version: String
    var artifactSHA256: String
    var installationPath: String
    var installedAt: String
    var pluginKey: String? = nil
}

struct NativeConnectorStateStore: Sendable {
    let stateURL: URL

    func load() throws -> NativeConnectorPersistentState {
        guard FileManager.default.fileExists(atPath: stateURL.path) else { return .empty }
        return try JSONDecoder().decode(
            NativeConnectorPersistentState.self,
            from: Data(contentsOf: stateURL)
        )
    }

    func save(_ state: NativeConnectorPersistentState) throws {
        try FileManager.default.createDirectory(
            at: stateURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(state).write(to: stateURL, options: .atomic)
    }
}

protocol NativeConnectorSecretStoring: Sendable {
    func load(account: String) throws -> Data?
    func save(_ value: Data, account: String) throws
    func delete(account: String) throws
}

enum NativeConnectorSecretStoreError: Error, Equatable, Sendable {
    case invalidAccount
}

struct NativeConnectorSecretStore: NativeConnectorSecretStoring, Sendable {
    static let productionService = "com.chatos.native-connector.credentials.v1"

    private let service: String
    private let broker: MacOSKeychainBrokerClient

    init(
        service: String = productionService,
        broker: MacOSKeychainBrokerClient = .init()
    ) {
        self.service = service
        self.broker = broker
    }

    func load(account: String) throws -> Data? {
        guard Self.valid(account) else { throw NativeConnectorSecretStoreError.invalidAccount }
        return try broker.load(service: service, account: account)
    }

    func save(_ value: Data, account: String) throws {
        guard Self.valid(account) else { throw NativeConnectorSecretStoreError.invalidAccount }
        try broker.save(value, service: service, account: account)
    }

    func delete(account: String) throws {
        guard Self.valid(account) else { throw NativeConnectorSecretStoreError.invalidAccount }
        try broker.delete(service: service, account: account)
    }

    private static func valid(_ account: String) -> Bool {
        !account.isEmpty
            && account.utf8.count <= 1_024
            && account == account.trimmingCharacters(in: .whitespacesAndNewlines)
            && !account.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }
}

struct NativeConnectorDeviceIdentity: Sendable {
    private static let account = "device-signing-key-v1"
    private let privateKey: Curve25519.Signing.PrivateKey

    init(secretStore: any NativeConnectorSecretStoring) throws {
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
