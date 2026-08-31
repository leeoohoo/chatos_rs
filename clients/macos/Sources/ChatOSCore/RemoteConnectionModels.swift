import Foundation

public enum RemoteAuthenticationType: String, Codable, CaseIterable, Sendable {
    case privateKey = "private_key"
    case privateKeyCertificate = "private_key_cert"
    case password
}

public enum RemoteHostKeyPolicy: String, Codable, CaseIterable, Sendable {
    case strict
    case acceptNew = "accept_new"
}

public struct RemoteConnection: Sendable, Equatable, Identifiable {
    public var id: String
    public var name: String
    public var host: String
    public var port: Int
    public var username: String
    public var authenticationType: RemoteAuthenticationType
    public var hasPassword: Bool
    public var hasPrivateKeyPath: Bool
    public var hasCertificatePath: Bool
    public var defaultRemotePath: String?
    public var hostKeyPolicy: RemoteHostKeyPolicy
    public var localConnectorDeviceID: String
    public var localConnectorWorkspaceID: String
    public var jumpEnabled: Bool
    public var jumpConnectionID: String?
    public var jumpHost: String?
    public var jumpPort: Int?
    public var jumpUsername: String?
    public var hasJumpPrivateKeyPath: Bool
    public var hasJumpCertificatePath: Bool
    public var hasJumpPassword: Bool
    public var lastActiveAt: Date?

    public init(
        id: String,
        name: String,
        host: String,
        port: Int,
        username: String,
        authenticationType: RemoteAuthenticationType,
        hasPassword: Bool,
        hasPrivateKeyPath: Bool,
        hasCertificatePath: Bool,
        defaultRemotePath: String?,
        hostKeyPolicy: RemoteHostKeyPolicy,
        localConnectorDeviceID: String,
        localConnectorWorkspaceID: String,
        jumpEnabled: Bool,
        jumpConnectionID: String?,
        jumpHost: String?,
        jumpPort: Int?,
        jumpUsername: String?,
        hasJumpPrivateKeyPath: Bool,
        hasJumpCertificatePath: Bool,
        hasJumpPassword: Bool,
        lastActiveAt: Date?
    ) {
        self.id = id
        self.name = name
        self.host = host
        self.port = port
        self.username = username
        self.authenticationType = authenticationType
        self.hasPassword = hasPassword
        self.hasPrivateKeyPath = hasPrivateKeyPath
        self.hasCertificatePath = hasCertificatePath
        self.defaultRemotePath = defaultRemotePath
        self.hostKeyPolicy = hostKeyPolicy
        self.localConnectorDeviceID = localConnectorDeviceID
        self.localConnectorWorkspaceID = localConnectorWorkspaceID
        self.jumpEnabled = jumpEnabled
        self.jumpConnectionID = jumpConnectionID
        self.jumpHost = jumpHost
        self.jumpPort = jumpPort
        self.jumpUsername = jumpUsername
        self.hasJumpPrivateKeyPath = hasJumpPrivateKeyPath
        self.hasJumpCertificatePath = hasJumpCertificatePath
        self.hasJumpPassword = hasJumpPassword
        self.lastActiveAt = lastActiveAt
    }
}

public struct RemoteConnectionDraft: Sendable, Equatable {
    public var name: String?
    public var host: String
    public var port: Int
    public var username: String
    public var authenticationType: RemoteAuthenticationType
    public var password: String?
    public var privateKeyPath: String?
    public var certificatePath: String?
    public var defaultRemotePath: String?
    public var hostKeyPolicy: RemoteHostKeyPolicy
    public var localConnectorDeviceID: String
    public var localConnectorWorkspaceID: String
    public var jumpEnabled: Bool
    public var jumpConnectionID: String?
    public var jumpHost: String?
    public var jumpPort: Int?
    public var jumpUsername: String?
    public var jumpPrivateKeyPath: String?
    public var jumpCertificatePath: String?
    public var jumpPassword: String?
    public var localCredentialReferenceID: String?

    public init(
        name: String?,
        host: String,
        port: Int,
        username: String,
        authenticationType: RemoteAuthenticationType,
        password: String?,
        privateKeyPath: String?,
        certificatePath: String?,
        defaultRemotePath: String?,
        hostKeyPolicy: RemoteHostKeyPolicy,
        localConnectorDeviceID: String,
        localConnectorWorkspaceID: String,
        jumpEnabled: Bool,
        jumpConnectionID: String?,
        jumpHost: String?,
        jumpPort: Int?,
        jumpUsername: String?,
        jumpPrivateKeyPath: String?,
        jumpCertificatePath: String?,
        jumpPassword: String?,
        localCredentialReferenceID: String? = nil
    ) {
        self.name = name
        self.host = host
        self.port = port
        self.username = username
        self.authenticationType = authenticationType
        self.password = password
        self.privateKeyPath = privateKeyPath
        self.certificatePath = certificatePath
        self.defaultRemotePath = defaultRemotePath
        self.hostKeyPolicy = hostKeyPolicy
        self.localConnectorDeviceID = localConnectorDeviceID
        self.localConnectorWorkspaceID = localConnectorWorkspaceID
        self.jumpEnabled = jumpEnabled
        self.jumpConnectionID = jumpConnectionID
        self.jumpHost = jumpHost
        self.jumpPort = jumpPort
        self.jumpUsername = jumpUsername
        self.jumpPrivateKeyPath = jumpPrivateKeyPath
        self.jumpCertificatePath = jumpCertificatePath
        self.jumpPassword = jumpPassword
        self.localCredentialReferenceID = localCredentialReferenceID
    }
}

public struct RemoteConnectionTestResult: Sendable, Equatable {
    public var success: Bool
    public var message: String?

    public init(success: Bool, message: String?) {
        self.success = success
        self.message = message
    }
}

public struct RemoteTerminalCommandResult: Sendable, Equatable {
    public var output: String
    public var error: String
    public var exitCode: Int32
    public var workingDirectory: String

    public init(
        output: String,
        error: String,
        exitCode: Int32,
        workingDirectory: String
    ) {
        self.output = output
        self.error = error
        self.exitCode = exitCode
        self.workingDirectory = workingDirectory
    }
}

public protocol RemoteTerminalCommandServicing: Sendable {
    func executeRemoteCommand(
        connectionID: String,
        command: String,
        workingDirectory: String
    ) async throws -> RemoteTerminalCommandResult
}

public struct RemoteVerificationChallenge: Error, Sendable, Equatable {
    public var prompt: String

    public init(prompt: String) {
        self.prompt = prompt
    }
}

public protocol RemoteConnectionServicing: Sendable {
    func listConnections() async throws -> [RemoteConnection]
    func createConnection(_ draft: RemoteConnectionDraft) async throws -> RemoteConnection
    func updateConnection(id: String, draft: RemoteConnectionDraft) async throws -> RemoteConnection
    func deleteConnection(id: String) async throws
    func testDraft(_ draft: RemoteConnectionDraft, verificationCode: String?) async throws -> RemoteConnectionTestResult
    func testSaved(id: String, verificationCode: String?) async throws -> RemoteConnectionTestResult
}
