import ChatOSCore
import Foundation

struct DesktopTicketDTO: Decodable, Sendable { var ticket: String }

struct DesktopTicketAuthDTO: Encodable {
    var cloudBaseURL: String
    var ticket: String
    var deviceName: String?

    enum CodingKeys: String, CodingKey {
        case cloudBaseURL = "cloud_base_url"
        case ticket
        case deviceName = "device_name"
    }
}
struct ConnectorStatusDTO: Decodable, Sendable {
    var configured: Bool
    var connectorRunning: Bool
    var developerMode: Bool?
    var cloudBaseURL: String?
    var userServiceBaseURL: String?
    var deviceID: String?
    var deviceName: String?
    var user: ConnectorUserDTO?
    var defaultWorkspaceID: String?
    var workspaces: [ConnectorWorkspaceDTO]

    enum CodingKeys: String, CodingKey {
        case configured
        case connectorRunning = "connector_running"
        case developerMode = "developer_mode"
        case cloudBaseURL = "cloud_base_url"
        case userServiceBaseURL = "user_service_base_url"
        case deviceID = "device_id"
        case deviceName = "device_name"
        case user
        case defaultWorkspaceID = "default_workspace_id"
        case workspaces
    }

    var domainModel: LocalConnectorStatus {
        LocalConnectorStatus(
            configured: configured,
            connectorRunning: connectorRunning,
            developerMode: developerMode ?? false,
            cloudBaseURL: cloudBaseURL,
            userServiceBaseURL: userServiceBaseURL,
            deviceID: deviceID,
            deviceName: deviceName,
            user: user?.domainModel,
            defaultWorkspaceID: defaultWorkspaceID,
            workspaces: workspaces.map(\.domainModel)
        )
    }
}

struct ConnectorUserDTO: Decodable, Sendable {
    var id: String
    var username: String
    var displayName: String?
    var role: String

    enum CodingKeys: String, CodingKey {
        case id, username, role
        case displayName = "display_name"
    }

    var domainModel: LocalConnectorUser {
        .init(id: id, username: username, displayName: displayName, role: role)
    }
}

struct ConnectorWorkspaceDTO: Decodable, Sendable {
    var id: String
    var alias: String
    var absoluteRoot: String
    var fingerprint: String
    var projectConfigTrusted: Bool?
    var projectConfigTrustStale: Bool?

    enum CodingKeys: String, CodingKey {
        case id, alias, fingerprint
        case absoluteRoot = "absolute_root"
        case projectConfigTrusted = "project_config_trusted"
        case projectConfigTrustStale = "project_config_trust_stale"
    }

    var domainModel: LocalConnectorWorkspace {
        .init(
            id: id,
            alias: alias,
            absoluteRoot: absoluteRoot,
            fingerprint: fingerprint,
            projectConfigTrusted: projectConfigTrusted,
            projectConfigTrustStale: projectConfigTrustStale
        )
    }
}

struct LocalRuntimeSettingsDTO: Codable, Sendable {
    var developerMode: Bool
    var developerCloudBaseURL: String
    var developerUserServiceBaseURL: String
    var developerChatOSWebURL: String

    enum CodingKeys: String, CodingKey {
        case developerMode = "developer_mode"
        case developerCloudBaseURL = "developer_cloud_base_url"
        case developerUserServiceBaseURL = "developer_user_service_base_url"
        case developerChatOSWebURL = "developer_chatos_web_url"
    }

    var domainModel: LocalConnectorRuntimeSettings {
        .init(
            developerMode: developerMode,
            developerCloudBaseURL: developerCloudBaseURL,
            developerUserServiceBaseURL: developerUserServiceBaseURL,
            developerChatOSWebURL: developerChatOSWebURL
        )
    }
}

struct UpdateRuntimeSettingsDTO: Encodable {
    var developerMode: Bool
    enum CodingKeys: String, CodingKey { case developerMode = "developer_mode" }
}

struct SystemPermissionsDTO: Decodable, Sendable {
    var platform: String
    var platformLabel: String
    var items: [SystemPermissionDTO]
    enum CodingKeys: String, CodingKey {
        case platform, items
        case platformLabel = "platform_label"
    }
    var domainModel: LocalConnectorSystemPermissions {
        .init(platform: platform, platformLabel: platformLabel, items: items.map(\.domainModel))
    }
}

struct SystemPermissionDTO: Decodable, Sendable {
    var id: String
    var label: String
    var summary: String
    var status: String
    var statusLabel: String
    var required: Bool
    var canRequest: Bool
    var requestLabel: String
    var note: String
    var lastError: String?
    enum CodingKeys: String, CodingKey {
        case id, label, summary, status, required, note
        case statusLabel = "status_label"
        case canRequest = "can_request"
        case requestLabel = "request_label"
        case lastError = "last_error"
    }
    var domainModel: LocalConnectorSystemPermission {
        .init(
            id: id, label: label, summary: summary, status: status,
            statusLabel: statusLabel, required: required, canRequest: canRequest,
            requestLabel: requestLabel, note: note, lastError: lastError
        )
    }
}
