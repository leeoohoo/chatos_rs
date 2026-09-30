import ChatOSCore
import Foundation

struct LocalRemoteConnectionRecord: Decodable, Sendable, Equatable {
    let connectionID: String
    let ownerUserID: String
    let name: String
    let host: String
    let port: UInt32
    let username: String
    let authenticationType: String
    let hasPassword: Bool
    let hasPrivateKeyPath: Bool
    let hasCertificatePath: Bool
    let defaultRemotePath: String?
    let hostKeyPolicy: String
    let localConnectorDeviceID: String
    let localConnectorWorkspaceID: String
    let jumpEnabled: Bool
    let jumpConnectionID: String?
    let jumpHost: String?
    let jumpPort: UInt32?
    let jumpUsername: String?
    let hasJumpPrivateKeyPath: Bool
    let hasJumpCertificatePath: Bool
    let hasJumpPassword: Bool
    let lastActiveAtUnixMs: Int64?
    let version: UInt64

    enum CodingKeys: String, CodingKey {
        case name, host, port, username, version
        case connectionID = "connection_id"
        case ownerUserID = "owner_user_id"
        case authenticationType = "authentication_type"
        case hasPassword = "has_password"
        case hasPrivateKeyPath = "has_private_key_path"
        case hasCertificatePath = "has_certificate_path"
        case defaultRemotePath = "default_remote_path"
        case hostKeyPolicy = "host_key_policy"
        case localConnectorDeviceID = "local_connector_device_id"
        case localConnectorWorkspaceID = "local_connector_workspace_id"
        case jumpEnabled = "jump_enabled"
        case jumpConnectionID = "jump_connection_id"
        case jumpHost = "jump_host"
        case jumpPort = "jump_port"
        case jumpUsername = "jump_username"
        case hasJumpPrivateKeyPath = "has_jump_private_key_path"
        case hasJumpCertificatePath = "has_jump_certificate_path"
        case hasJumpPassword = "has_jump_password"
        case lastActiveAtUnixMs = "last_active_at_unix_ms"
    }
}

struct NativeLocalAgentRemoteConnectionClient: Sendable {
    private let host: any LocalAgentHostClientServicing

    init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    func list(ownerUserID: String) async throws -> [LocalRemoteConnectionRecord] {
        let response: ListResult = try await host.request(OwnerCommand(
            type: "list_remote_connections",
            ownerUserID: ownerUserID
        ))
        guard response.type == "remote_connections" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return response.connections
    }

    func get(ownerUserID: String, connectionID: String) async throws -> LocalRemoteConnectionRecord? {
        let response: ConnectionResult = try await host.request(IdentityCommand(
            type: "get_remote_connection",
            ownerUserID: ownerUserID,
            connectionID: connectionID
        ))
        guard response.type == "remote_connection" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return response.connection
    }

    func create(
        ownerUserID: String,
        draft: RemoteConnectionDraft
    ) async throws -> LocalRemoteConnectionRecord {
        let response: ConnectionResult = try await host.request(CreateCommand(
            type: "create_remote_connection",
            ownerUserID: ownerUserID,
            spec: Spec(draft: draft)
        ))
        return try requireConnection(response)
    }

    func update(
        ownerUserID: String,
        connectionID: String,
        expectedVersion: UInt64,
        draft: RemoteConnectionDraft
    ) async throws -> LocalRemoteConnectionRecord {
        let response: ConnectionResult = try await host.request(UpdateCommand(
            type: "update_remote_connection",
            ownerUserID: ownerUserID,
            connectionID: connectionID,
            expectedVersion: expectedVersion,
            spec: Spec(draft: draft)
        ))
        return try requireConnection(response)
    }

    func delete(
        ownerUserID: String,
        connectionID: String,
        expectedVersion: UInt64
    ) async throws {
        let response: DeletedResult = try await host.request(DeleteCommand(
            type: "delete_remote_connection",
            ownerUserID: ownerUserID,
            connectionID: connectionID,
            expectedVersion: expectedVersion
        ))
        guard response.type == "remote_connection_deleted",
              response.connectionID == connectionID else {
            throw NativeLocalAgentHostError.invalidResponse
        }
    }

    private func requireConnection(_ response: ConnectionResult) throws -> LocalRemoteConnectionRecord {
        guard response.type == "remote_connection", let connection = response.connection else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return connection
    }
}

private struct Spec: Encodable, Sendable {
    let name: String?
    let host: String
    let port: UInt32
    let username: String
    let authenticationType: String
    let defaultRemotePath: String?
    let hostKeyPolicy: String
    let localConnectorDeviceID: String
    let localConnectorWorkspaceID: String
    let jumpEnabled: Bool
    let jumpConnectionID: String?
    let jumpHost: String?
    let jumpPort: UInt32?
    let jumpUsername: String?

    init(draft: RemoteConnectionDraft) {
        name = draft.name
        host = draft.host
        port = UInt32(clamping: draft.port)
        username = draft.username
        authenticationType = draft.authenticationType.rawValue
        defaultRemotePath = draft.defaultRemotePath
        hostKeyPolicy = draft.hostKeyPolicy.rawValue
        localConnectorDeviceID = draft.localConnectorDeviceID
        localConnectorWorkspaceID = draft.localConnectorWorkspaceID
        jumpEnabled = draft.jumpEnabled
        jumpConnectionID = draft.jumpConnectionID
        jumpHost = draft.jumpHost
        jumpPort = draft.jumpPort.map(UInt32.init(clamping:))
        jumpUsername = draft.jumpUsername
    }

    enum CodingKeys: String, CodingKey {
        case name, host, port, username
        case authenticationType = "authentication_type"
        case defaultRemotePath = "default_remote_path"
        case hostKeyPolicy = "host_key_policy"
        case localConnectorDeviceID = "local_connector_device_id"
        case localConnectorWorkspaceID = "local_connector_workspace_id"
        case jumpEnabled = "jump_enabled"
        case jumpConnectionID = "jump_connection_id"
        case jumpHost = "jump_host"
        case jumpPort = "jump_port"
        case jumpUsername = "jump_username"
    }
}

private struct OwnerCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    enum CodingKeys: String, CodingKey { case type; case ownerUserID = "owner_user_id" }
}

private struct IdentityCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let connectionID: String
    enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case connectionID = "connection_id"
    }
}

private struct CreateCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let spec: Spec
    enum CodingKeys: String, CodingKey { case type, spec; case ownerUserID = "owner_user_id" }
}

private struct UpdateCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let connectionID: String
    let expectedVersion: UInt64
    let spec: Spec
    enum CodingKeys: String, CodingKey {
        case type, spec
        case ownerUserID = "owner_user_id"
        case connectionID = "connection_id"
        case expectedVersion = "expected_version"
    }
}

private struct DeleteCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let connectionID: String
    let expectedVersion: UInt64
    enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case connectionID = "connection_id"
        case expectedVersion = "expected_version"
    }
}

private struct ListResult: Decodable, Sendable {
    let type: String
    let connections: [LocalRemoteConnectionRecord]
}

private struct ConnectionResult: Decodable, Sendable {
    let type: String
    let connection: LocalRemoteConnectionRecord?
}

private struct DeletedResult: Decodable, Sendable {
    let type: String
    let connectionID: String
    enum CodingKeys: String, CodingKey { case type; case connectionID = "connection_id" }
}
