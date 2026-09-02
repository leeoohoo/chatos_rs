import ChatOSCore
import Foundation

struct ModelCatalogDTO: Decodable, Sendable {
    var items: [LocalModelConfigDTO]
    var settings: ModelSettingsDTO
    var domainModel: LocalConnectorModelCatalog {
        .init(items: items.map(\.domainModel), settings: settings.domainModel)
    }
}

struct LocalModelConfigDTO: Decodable, Sendable {
    var id: String
    var name: String
    var provider: String
    var modelName: String
    var enabled: Bool
    var hasAPIKey: Bool
    var supportsImages: Bool
    var supportsReasoning: Bool
    enum CodingKeys: String, CodingKey {
        case id, name, provider, enabled
        case modelName = "model_name"
        case hasAPIKey = "has_api_key"
        case supportsImages = "supports_images"
        case supportsReasoning = "supports_reasoning"
    }
    var domainModel: LocalConnectorModelConfig {
        .init(
            id: id, name: name, provider: provider, modelName: modelName,
            enabled: enabled, hasAPIKey: hasAPIKey,
            supportsImages: supportsImages, supportsReasoning: supportsReasoning
        )
    }
}

struct ModelSettingsDTO: Decodable, Sendable {
    var modelRequestMaxRetries: Int?
    var commandApprovalModelConfigID: String?
    var commandApprovalThinkingLevel: String?
    enum CodingKeys: String, CodingKey {
        case modelRequestMaxRetries = "model_request_max_retries"
        case commandApprovalModelConfigID = "command_approval_model_config_id"
        case commandApprovalThinkingLevel = "command_approval_thinking_level"
    }
    var domainModel: LocalConnectorModelSettings {
        .init(
            modelRequestMaxRetries: modelRequestMaxRetries,
            commandApprovalModelConfigID: commandApprovalModelConfigID,
            commandApprovalThinkingLevel: commandApprovalThinkingLevel
        )
    }
}

struct SandboxCapabilitiesDTO: Decodable, Sendable { var backends: [SandboxBackendDTO] }

struct SandboxBackendDTO: Decodable, Sendable {
    var backend: String
    var status: String
    var selectable: Bool
    var filesystemIsolation: Bool
    var networkIsolation: Bool
    var processTreeControl: Bool
    var message: String
    enum CodingKeys: String, CodingKey {
        case backend, status, selectable, message
        case filesystemIsolation = "filesystem_isolation"
        case networkIsolation = "network_isolation"
        case processTreeControl = "process_tree_control"
    }
    var domainModel: LocalConnectorSandboxBackend {
        .init(
            backend: backend, status: status, selectable: selectable,
            filesystemIsolation: filesystemIsolation, networkIsolation: networkIsolation,
            processTreeControl: processTreeControl, message: message
        )
    }
}

struct SandboxSettingsDTO: Decodable, Sendable {
    var enabled: Bool
    var defaultBackend: String
    var defaultPermissionProfileID: String
    var defaultPermissionProfileName: String
    var defaultApprovalPolicy: String
    var defaultApprovalReviewer: String
    var defaultNetworkAccess: String
    var permissionConfigurationError: String?
    var policyRevision: String?
    enum CodingKeys: String, CodingKey {
        case enabled
        case defaultBackend = "default_backend"
        case defaultPermissionProfileID = "default_permission_profile_id"
        case defaultPermissionProfileName = "default_permission_profile_name"
        case defaultApprovalPolicy = "default_approval_policy"
        case defaultApprovalReviewer = "default_approval_reviewer"
        case defaultNetworkAccess = "default_network_access"
        case permissionConfigurationError = "permission_configuration_error"
        case policyRevision = "policy_revision"
    }
    var domainModel: LocalConnectorSandboxSettings {
        .init(
            enabled: enabled, defaultBackend: defaultBackend,
            defaultPermissionProfileID: defaultPermissionProfileID,
            defaultPermissionProfileName: defaultPermissionProfileName,
            defaultApprovalPolicy: defaultApprovalPolicy,
            defaultApprovalReviewer: defaultApprovalReviewer,
            defaultNetworkAccess: defaultNetworkAccess,
            permissionConfigurationError: permissionConfigurationError,
            policyRevision: policyRevision
        )
    }
}

struct UpdateSandboxSettingsDTO: Encodable {
    var enabled: Bool?
    var defaultPermissionProfileID: String?
    var defaultApprovalPolicy: String?
    var defaultApprovalReviewer: String?
    var defaultNetworkAccess: String?
    var riskAcknowledged: Bool
    enum CodingKeys: String, CodingKey {
        case enabled
        case defaultPermissionProfileID = "default_permission_profile_id"
        case defaultApprovalPolicy = "default_approval_policy"
        case defaultApprovalReviewer = "default_approval_reviewer"
        case defaultNetworkAccess = "default_network_access"
        case riskAcknowledged = "risk_acknowledged"
    }
}

struct PluginCatalogDTO: Decodable, Sendable { var items: [PluginDTO] }

struct PluginDTO: Decodable, Sendable {
    var pluginID: String
    var displayName: String
    var description: String
    var category: String
    var publisher: String
    var latestVersion: String
    var installation: PluginInstallationDTO?
    var updateAvailable: Bool
    var installAvailable: Bool
    var preference: PluginPreferenceDTO?
    var hasUI: Bool?
    enum CodingKeys: String, CodingKey {
        case description, category, publisher, installation, preference
        case pluginID = "plugin_id"
        case displayName = "display_name"
        case latestVersion = "latest_version"
        case updateAvailable = "update_available"
        case installAvailable = "install_available"
        case hasUI = "has_ui"
    }
    var domainModel: LocalConnectorPlugin {
        .init(
            pluginID: pluginID, displayName: displayName, description: description,
            category: category, publisher: publisher, latestVersion: latestVersion,
            installed: installation != nil, updateAvailable: updateAvailable,
            installAvailable: installAvailable, enabled: preference?.enabled ?? true,
            hasUI: hasUI
        )
    }
}

struct PluginInstallationDTO: Decodable, Sendable { var pluginID: String? = nil }
struct PluginPreferenceDTO: Decodable, Sendable { var enabled: Bool }
struct UninstallPluginDTO: Encodable {
    var acknowledgePluginDataRemoval: Bool
    enum CodingKeys: String, CodingKey {
        case acknowledgePluginDataRemoval = "acknowledge_plugin_data_removal"
    }
}
struct UpdatePluginPreferenceDTO: Encodable { var enabled: Bool }
