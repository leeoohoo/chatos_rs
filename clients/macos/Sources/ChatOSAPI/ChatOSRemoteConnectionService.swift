import ChatOSCore
import Foundation

public struct ChatOSRemoteConnectionService: RemoteConnectionServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func listConnections() async throws -> [RemoteConnection] {
        let response: [RemoteConnectionDTO] = try await client.request("/remote-connections")
        return response.map(\.domainModel)
    }

    public func createConnection(_ draft: RemoteConnectionDraft) async throws -> RemoteConnection {
        let response: RemoteConnectionDTO = try await client.request(
            "/remote-connections",
            method: "POST",
            body: try JSONEncoder().encode(RemoteConnectionDraftDTO(draft: draft))
        )
        return response.domainModel
    }

    public func updateConnection(
        id: String,
        draft: RemoteConnectionDraft
    ) async throws -> RemoteConnection {
        let response: RemoteConnectionDTO = try await client.request(
            "/remote-connections/\(id.pathEncoded)",
            method: "PUT",
            body: try JSONEncoder().encode(RemoteConnectionDraftDTO(draft: draft))
        )
        return response.domainModel
    }

    public func deleteConnection(id: String) async throws {
        let _: MutationDTO = try await client.request(
            "/remote-connections/\(id.pathEncoded)",
            method: "DELETE"
        )
    }

    public func testDraft(
        _ draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        try await performTest(
            endpoint: "/remote-connections/test",
            body: try JSONEncoder().encode(RemoteConnectionDraftDTO(draft: draft)),
            verificationCode: verificationCode
        )
    }

    public func testSaved(
        id: String,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        try await performTest(
            endpoint: "/remote-connections/\(id.pathEncoded)/test",
            body: nil,
            verificationCode: verificationCode
        )
    }

    private func performTest(
        endpoint: String,
        body: Data?,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        let headers = verificationCode?.trimmedNonEmpty.map {
            ["x-remote-verification-code": $0]
        } ?? [:]
        do {
            let response: RemoteConnectionTestDTO = try await client.request(
                endpoint,
                method: "POST",
                body: body,
                additionalHeaders: headers
            )
            return .init(
                success: response.success
                    ?? (response.status == "ok" || response.status == "success"),
                message: response.message
            )
        } catch let ChatOSAPIError.serverDetail(_, message, code, prompt)
            where code?.lowercased() == "second_factor_required" {
            throw RemoteVerificationChallenge(prompt: prompt?.trimmedNonEmpty ?? message)
        }
    }
}

private struct RemoteConnectionDTO: Decodable {
    var id: String
    var name: String?
    var host: String?
    var port: Int?
    var username: String?
    var authenticationType: RemoteAuthenticationType?
    var hasPassword: Bool?
    var hasPrivateKeyPath: Bool?
    var hasCertificatePath: Bool?
    var defaultRemotePath: String?
    var hostKeyPolicy: RemoteHostKeyPolicy?
    var localConnectorDeviceID: String?
    var localConnectorWorkspaceID: String?
    var jumpEnabled: Bool?
    var jumpConnectionID: String?
    var jumpHost: String?
    var jumpPort: Int?
    var jumpUsername: String?
    var hasJumpPrivateKeyPath: Bool?
    var hasJumpCertificatePath: Bool?
    var hasJumpPassword: Bool?
    var lastActiveAt: String?

    enum CodingKeys: String, CodingKey {
        case id, name, host, port, username
        case authenticationType = "auth_type"
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
        case lastActiveAt = "last_active_at"
    }

    var domainModel: RemoteConnection {
        RemoteConnection(
            id: id,
            name: name?.trimmedNonEmpty ?? "未命名远端",
            host: host ?? "",
            port: port ?? 22,
            username: username ?? "",
            authenticationType: authenticationType ?? .privateKey,
            hasPassword: hasPassword ?? false,
            hasPrivateKeyPath: hasPrivateKeyPath ?? false,
            hasCertificatePath: hasCertificatePath ?? false,
            defaultRemotePath: defaultRemotePath?.trimmedNonEmpty,
            hostKeyPolicy: hostKeyPolicy ?? .strict,
            localConnectorDeviceID: localConnectorDeviceID ?? "",
            localConnectorWorkspaceID: localConnectorWorkspaceID ?? "",
            jumpEnabled: jumpEnabled ?? false,
            jumpConnectionID: jumpConnectionID?.trimmedNonEmpty,
            jumpHost: jumpHost?.trimmedNonEmpty,
            jumpPort: jumpPort,
            jumpUsername: jumpUsername?.trimmedNonEmpty,
            hasJumpPrivateKeyPath: hasJumpPrivateKeyPath ?? false,
            hasJumpCertificatePath: hasJumpCertificatePath ?? false,
            hasJumpPassword: hasJumpPassword ?? false,
            lastActiveAt: APIDateParser.parse(lastActiveAt)
        )
    }
}

private struct RemoteConnectionDraftDTO: Encodable {
    var name: String?
    var host: String
    var port: Int
    var username: String
    var authenticationType: RemoteAuthenticationType
    var password: String?
    var privateKeyPath: String?
    var certificatePath: String?
    var defaultRemotePath: String?
    var hostKeyPolicy: RemoteHostKeyPolicy
    var localConnectorDeviceID: String
    var localConnectorWorkspaceID: String
    var jumpEnabled: Bool
    var jumpConnectionID: String?
    var jumpHost: String?
    var jumpPort: Int?
    var jumpUsername: String?
    var jumpPrivateKeyPath: String?
    var jumpCertificatePath: String?
    var jumpPassword: String?

    init(draft: RemoteConnectionDraft) {
        name = draft.name
        host = draft.host
        port = draft.port
        username = draft.username
        authenticationType = draft.authenticationType
        password = draft.password
        privateKeyPath = draft.privateKeyPath
        certificatePath = draft.certificatePath
        defaultRemotePath = draft.defaultRemotePath
        hostKeyPolicy = draft.hostKeyPolicy
        localConnectorDeviceID = draft.localConnectorDeviceID
        localConnectorWorkspaceID = draft.localConnectorWorkspaceID
        jumpEnabled = draft.jumpEnabled
        jumpConnectionID = draft.jumpConnectionID
        jumpHost = draft.jumpHost
        jumpPort = draft.jumpPort
        jumpUsername = draft.jumpUsername
        jumpPrivateKeyPath = draft.jumpPrivateKeyPath
        jumpCertificatePath = draft.jumpCertificatePath
        jumpPassword = draft.jumpPassword
    }

    enum CodingKeys: String, CodingKey {
        case name, host, port, username, password
        case authenticationType = "auth_type"
        case privateKeyPath = "private_key_path"
        case certificatePath = "certificate_path"
        case defaultRemotePath = "default_remote_path"
        case hostKeyPolicy = "host_key_policy"
        case localConnectorDeviceID = "local_connector_device_id"
        case localConnectorWorkspaceID = "local_connector_workspace_id"
        case jumpEnabled = "jump_enabled"
        case jumpConnectionID = "jump_connection_id"
        case jumpHost = "jump_host"
        case jumpPort = "jump_port"
        case jumpUsername = "jump_username"
        case jumpPrivateKeyPath = "jump_private_key_path"
        case jumpCertificatePath = "jump_certificate_path"
        case jumpPassword = "jump_password"
    }
}

private struct RemoteConnectionTestDTO: Decodable {
    var success: Bool?
    var status: String?
    var message: String?
}

private struct MutationDTO: Decodable {
    var success: Bool?
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }

    var pathEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .remotePathComponentAllowed) ?? self
    }
}

private extension CharacterSet {
    static let remotePathComponentAllowed: CharacterSet = {
        var set = CharacterSet.urlPathAllowed
        set.remove(charactersIn: "/?#")
        return set
    }()
}
