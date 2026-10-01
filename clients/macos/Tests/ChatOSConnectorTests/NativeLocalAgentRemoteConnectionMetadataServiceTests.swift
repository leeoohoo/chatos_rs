@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentRemoteConnectionMetadataServiceTests: XCTestCase {
    func testMetadataLifecycleUsesOwnerAndOptimisticVersionsWithoutSecrets() async throws {
        let host = RemoteConnectionHostStub()
        let service = NativeLocalAgentRemoteConnectionMetadataService(host: host)
        await service.configure(ownerUserID: "user-1")

        let created = try await service.createConnection(Self.draft(name: "Production"))
        XCTAssertEqual(created.id, "remote-1")
        var command = try await host.lastCommand()
        XCTAssertEqual(command.type, "create_remote_connection")
        XCTAssertEqual(command.ownerUserID, "user-1")
        XCTAssertFalse(command.json.contains("top-secret"))

        let updated = try await service.updateConnection(
            id: created.id,
            draft: Self.draft(name: "Renamed")
        )
        XCTAssertEqual(updated.name, "Renamed")
        command = try await host.lastCommand()
        XCTAssertEqual(command.expectedVersion, 1)

        try await service.deleteConnection(id: created.id)
        command = try await host.lastCommand()
        XCTAssertEqual(command.type, "delete_remote_connection")
        XCTAssertEqual(command.expectedVersion, 2)
    }

    func testResetRemovesAccountScope() async throws {
        let host = RemoteConnectionHostStub()
        let service = NativeLocalAgentRemoteConnectionMetadataService(host: host)
        await service.configure(ownerUserID: "user-1")
        await service.reset()
        do {
            _ = try await service.listConnections()
            XCTFail("reset service should reject access")
        } catch let error as NativeLocalAgentRemoteConnectionMetadataError {
            XCTAssertEqual(error, .notConfigured)
        }

        await service.configure(ownerUserID: "user-2")
        _ = try await service.listConnections()
        let command = try await host.lastCommand()
        XCTAssertEqual(command.ownerUserID, "user-2")
    }

    private static func draft(name: String) -> RemoteConnectionDraft {
        RemoteConnectionDraft(
            name: name,
            host: "server.example.com",
            port: 22,
            username: "deploy",
            authenticationType: .password,
            password: "top-secret",
            privateKeyPath: nil,
            certificatePath: nil,
            defaultRemotePath: "/srv/app",
            hostKeyPolicy: .strict,
            localConnectorDeviceID: "device-local",
            localConnectorWorkspaceID: "workspace-local",
            jumpEnabled: false,
            jumpConnectionID: nil,
            jumpHost: nil,
            jumpPort: nil,
            jumpUsername: nil,
            jumpPrivateKeyPath: nil,
            jumpCertificatePath: nil,
            jumpPassword: nil
        )
    }
}

private actor RemoteConnectionHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []
    private var version: UInt64 = 1
    private var name = "Production"

    func start(ownerUserID _: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "create_remote_connection":
            version = 1
            name = "Production"
            return try connectionResult()
        case "get_remote_connection":
            return try connectionResult()
        case "update_remote_connection":
            version += 1
            name = "Renamed"
            return try connectionResult()
        case "delete_remote_connection":
            return try json([
                "type": "remote_connection_deleted",
                "connection_id": "remote-1",
            ])
        case "list_remote_connections":
            return try json(["type": "remote_connections", "connections": []])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    func lastCommand() throws -> RemoteCommandSnapshot {
        let data = try XCTUnwrap(commands.last)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        return RemoteCommandSnapshot(
            type: object["type"] as? String,
            ownerUserID: object["owner_user_id"] as? String,
            expectedVersion: (object["expected_version"] as? NSNumber)?.uint64Value,
            json: String(decoding: data, as: UTF8.self)
        )
    }

    private func connectionResult() throws -> Data {
        try json([
            "type": "remote_connection",
            "connection": [
                "connection_id": "remote-1",
                "owner_user_id": "user-1",
                "name": name,
                "host": "server.example.com",
                "port": 22,
                "username": "deploy",
                "authentication_type": "password",
                "has_password": false,
                "has_private_key_path": false,
                "has_certificate_path": false,
                "host_key_policy": "strict",
                "local_connector_device_id": "device-local",
                "local_connector_workspace_id": "workspace-local",
                "jump_enabled": false,
                "has_jump_private_key_path": false,
                "has_jump_certificate_path": false,
                "has_jump_password": false,
                "version": version,
                "created_at_unix_ms": 1,
                "updated_at_unix_ms": 2,
            ] as [String: Any],
        ])
    }

    private func json(_ value: [String: Any]) throws -> Data {
        try JSONSerialization.data(withJSONObject: value)
    }
}

private struct RemoteCommandSnapshot: Sendable {
    let type: String?
    let ownerUserID: String?
    let expectedVersion: UInt64?
    let json: String
}
