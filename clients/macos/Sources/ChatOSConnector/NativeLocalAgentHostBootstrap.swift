// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Darwin
import Foundation

public enum NativeLocalAgentHostBootstrapError: Error, Equatable, Sendable {
    case invalidConfiguration(String)
    case missingCredential(String)
    case malformedCredential(String)
    case privateDirectoryRequired
    case socketPathTooLong
    case encodingFailed
}

public enum NativeLocalAgentPostgresTLSMode: String, Codable, Sendable {
    case verifyFull = "verify_full"
}

public struct NativeLocalAgentPostgresCredential: Codable, Sendable, CustomDebugStringConvertible {
    public let host: String
    public let port: UInt16
    public let database: String
    public let tlsMode: NativeLocalAgentPostgresTLSMode
    public let username: String
    public let password: String

    public init(
        host: String,
        port: UInt16 = 5432,
        database: String,
        tlsMode: NativeLocalAgentPostgresTLSMode = .verifyFull,
        username: String,
        password: String
    ) {
        self.host = host
        self.port = port
        self.database = database
        self.tlsMode = tlsMode
        self.username = username
        self.password = password
    }

    private enum CodingKeys: String, CodingKey {
        case host, port, database, username, password
        case tlsMode = "tls_mode"
    }

    public var debugDescription: String {
        "NativeLocalAgentPostgresCredential(host: [REDACTED], port: \(port), database: [REDACTED], tlsMode: \(tlsMode.rawValue), username: [REDACTED], password: [REDACTED])"
    }
}

public enum NativeLocalAgentStorageBootstrap: Sendable {
    case sqlite(databaseURL: URL, encryptionSecretReference: String)
    case postgres(connectionSecretReference: String)
}

public struct NativeLocalAgentHostBootstrapSettings: Sendable {
    public static var defaultRuntimeDirectory: URL {
        URL(fileURLWithPath: "/tmp/chatos-la-\(geteuid())", isDirectory: true)
    }

    public let executableURL: URL
    public let accountID: String
    public let deviceID: String
    public let runtimeDirectory: URL
    public let attachmentGrantDirectory: URL
    public let platformStateDirectory: URL
    public let modelGatewayBaseURL: URL
    public let memoryEngineBaseURL: URL
    public let memorySourceID: String
    public let storage: NativeLocalAgentStorageBootstrap

    public init(
        executableURL: URL,
        accountID: String,
        deviceID: String,
        runtimeDirectory: URL = defaultRuntimeDirectory,
        attachmentGrantDirectory: URL,
        platformStateDirectory: URL,
        modelGatewayBaseURL: URL,
        memoryEngineBaseURL: URL,
        memorySourceID: String = "local-agent",
        storage: NativeLocalAgentStorageBootstrap
    ) {
        self.executableURL = executableURL
        self.accountID = accountID
        self.deviceID = deviceID
        self.runtimeDirectory = runtimeDirectory
        self.attachmentGrantDirectory = attachmentGrantDirectory
        self.platformStateDirectory = platformStateDirectory
        self.modelGatewayBaseURL = modelGatewayBaseURL
        self.memoryEngineBaseURL = memoryEngineBaseURL
        self.memorySourceID = memorySourceID
        self.storage = storage
    }
}

public struct NativeLocalAgentHostBootstrapBuilder: Sendable {
    public static let modelAccessTokenReference = "model-access-token"
    public static let providerContextKeyReference = "provider-context-key"

    private let credentials: NativeLocalAgentCredentialStore

    public init(credentials: NativeLocalAgentCredentialStore) {
        self.credentials = credentials
    }

    public func makeConfiguration(
        settings: NativeLocalAgentHostBootstrapSettings
    ) async throws -> NativeLocalAgentHostLaunchConfiguration {
        try validate(settings)
        try ensurePrivateDirectory(settings.runtimeDirectory)
        try ensurePrivateDirectory(settings.attachmentGrantDirectory)
        try ensurePrivateDirectory(settings.platformStateDirectory)
        let launchID = "launch-\(UUID().uuidString.lowercased())"
        let workerID = "worker-\(UUID().uuidString.lowercased())"
        let socketURL = settings.runtimeDirectory
            .appendingPathComponent("agent-\(UUID().uuidString.lowercased()).sock")
        guard socketURL.path.utf8CString.count <= MemoryLayout.size(ofValue: sockaddr_un().sun_path)
        else {
            throw NativeLocalAgentHostBootstrapError.socketPathTooLong
        }

        let modelTokenData = try await requiredCredential(
            settings.accountID,
            Self.modelAccessTokenReference
        )
        let providerKey = try await requiredCredential(
            settings.accountID,
            Self.providerContextKeyReference
        )
        guard providerKey.count == 32 else {
            throw NativeLocalAgentHostBootstrapError.malformedCredential(
                Self.providerContextKeyReference
            )
        }
        let modelToken = try strictString(
            modelTokenData,
            reference: Self.modelAccessTokenReference
        )

        let storageProfile: [String: Any]
        let storageCredentials: [String: Any]
        switch settings.storage {
        case let .sqlite(databaseURL, encryptionSecretReference):
            guard databaseURL.isFileURL, databaseURL.path.hasPrefix("/") else {
                throw NativeLocalAgentHostBootstrapError.invalidConfiguration(
                    "SQLite database path must be absolute"
                )
            }
            let key = try await requiredCredential(settings.accountID, encryptionSecretReference)
            guard key.count == 32 else {
                throw NativeLocalAgentHostBootstrapError.malformedCredential(
                    encryptionSecretReference
                )
            }
            storageProfile = [
                "backend": "sqlite",
                "database_path": databaseURL.path,
                "encryption_secret": encryptionSecretReference,
            ]
            storageCredentials = [
                "backend": "sqlite",
                "encryption_secret_reference": encryptionSecretReference,
                "encryption_key_base64": key.base64EncodedString(),
            ]
        case let .postgres(connectionSecretReference):
            let envelopeData = try await requiredCredential(settings.accountID, connectionSecretReference)
            let envelope: NativeLocalAgentPostgresCredential
            do {
                envelope = try JSONDecoder().decode(
                    NativeLocalAgentPostgresCredential.self,
                    from: envelopeData
                )
            } catch {
                throw NativeLocalAgentHostBootstrapError.malformedCredential(
                    connectionSecretReference
                )
            }
            try validate(envelope)
            storageProfile = [
                "backend": "postgres",
                "connection_secret": connectionSecretReference,
            ]
            storageCredentials = [
                "backend": "postgres",
                "connection_secret_reference": connectionSecretReference,
                "host": envelope.host,
                "port": envelope.port,
                "database": envelope.database,
                "tls_mode": envelope.tlsMode.rawValue,
                "username": envelope.username,
                "password": envelope.password,
            ]
        }

        let request: [String: Any] = [
            "protocol_version": localAgentHostLaunchProtocolVersion,
            "launch_id": launchID,
            "owner_user_id": settings.accountID,
            "device_id": settings.deviceID,
            "worker_id": workerID,
            "ipc_endpoint": ["transport": "unix_socket", "path": socketURL.path],
            "attachment_grant_directory": settings.attachmentGrantDirectory.path,
            "platform_state_directory": settings.platformStateDirectory.path,
            "model_gateway_base_url": settings.modelGatewayBaseURL.absoluteString,
            "memory_engine_base_url": settings.memoryEngineBaseURL.absoluteString,
            "memory_source_id": settings.memorySourceID,
            "storage_profile": storageProfile,
            "credentials": [
                "model_access_token": modelToken,
                "provider_context_key_base64": providerKey.base64EncodedString(),
                "storage": storageCredentials,
            ],
        ]
        let data: Data
        do {
            data = try JSONSerialization.data(withJSONObject: request, options: [.sortedKeys])
        } catch {
            throw NativeLocalAgentHostBootstrapError.encodingFailed
        }
        return try NativeLocalAgentHostLaunchConfiguration(
            executableURL: settings.executableURL,
            launchID: launchID,
            expectedClientEndpoint: socketURL.path,
            launchRequestJSON: data
        )
    }

    private func requiredCredential(_ accountID: String, _ reference: String) async throws -> Data {
        guard let data = try await credentials.load(accountID: accountID, reference: reference)
        else {
            throw NativeLocalAgentHostBootstrapError.missingCredential(reference)
        }
        return data
    }

    private func validate(_ settings: NativeLocalAgentHostBootstrapSettings) throws {
        for (name, value) in [
            ("accountID", settings.accountID),
            ("deviceID", settings.deviceID),
            ("memorySourceID", settings.memorySourceID),
        ] where !validIdentity(value) {
            throw NativeLocalAgentHostBootstrapError.invalidConfiguration(name)
        }
        try validateServiceURL(settings.modelGatewayBaseURL, field: "modelGatewayBaseURL")
        try validateServiceURL(settings.memoryEngineBaseURL, field: "memoryEngineBaseURL")
    }

    private func validate(_ credential: NativeLocalAgentPostgresCredential) throws {
        guard validIdentity(credential.host),
              validIdentity(credential.database),
              validIdentity(credential.username),
              !credential.password.isEmpty,
              credential.password == credential.password.trimmingCharacters(in: .whitespacesAndNewlines)
        else {
            throw NativeLocalAgentHostBootstrapError.malformedCredential("postgres")
        }
    }

    private func validateServiceURL(_ url: URL, field: String) throws {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
              ["http", "https"].contains(components.scheme),
              components.host != nil,
              components.user == nil,
              components.password == nil,
              components.query == nil,
              components.fragment == nil
        else {
            throw NativeLocalAgentHostBootstrapError.invalidConfiguration(field)
        }
    }

    private func ensurePrivateDirectory(_ url: URL) throws {
        guard url.isFileURL, url.path.hasPrefix("/") else {
            throw NativeLocalAgentHostBootstrapError.privateDirectoryRequired
        }
        try FileManager.default.createDirectory(
            at: url,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: url.path
        )
        var metadata = stat()
        guard lstat(url.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFDIR,
              metadata.st_uid == geteuid(),
              metadata.st_mode & 0o077 == 0
        else {
            throw NativeLocalAgentHostBootstrapError.privateDirectoryRequired
        }
    }

    private func strictString(_ data: Data, reference: String) throws -> String {
        guard let value = String(data: data, encoding: .utf8),
              !value.isEmpty,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines)
        else {
            throw NativeLocalAgentHostBootstrapError.malformedCredential(reference)
        }
        return value
    }

    private func validIdentity(_ value: String) -> Bool {
        !value.isEmpty
            && value.count <= 512
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }
}
