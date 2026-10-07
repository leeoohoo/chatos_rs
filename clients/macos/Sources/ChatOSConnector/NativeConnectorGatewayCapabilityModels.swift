// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

struct GatewayAgentPromptBundleDTO: Decodable, Sendable {
    var bundleVersion: Int64
    var updatedAt: String
    var prompts: [GatewayAgentPromptDTO]

    enum CodingKeys: String, CodingKey {
        case prompts
        case bundleVersion = "bundle_version"
        case updatedAt = "updated_at"
    }
}

struct GatewayAgentPromptDTO: Decodable, Sendable {
    var agentKey: String
    var vendor: String
    var content: String
    var revision: Int64
    var checksum: String
    var publishedAt: String

    enum CodingKeys: String, CodingKey {
        case vendor, content, revision, checksum
        case agentKey = "agent_key"
        case publishedAt = "published_at"
    }
}

struct GatewayAgentCapabilityDTO: Decodable, Sendable {
    var agentKey: String
    var ownerUserID: String
    var policyRevision: String
    var agentEnabled: Bool
    var mcps: [GatewayResolvedMCPDTO]
    var plugins: [GatewayResolvedPluginDTO]

    init(
        agentKey: String,
        ownerUserID: String,
        policyRevision: String,
        agentEnabled: Bool,
        mcps: [GatewayResolvedMCPDTO] = [],
        plugins: [GatewayResolvedPluginDTO] = []
    ) {
        self.agentKey = agentKey
        self.ownerUserID = ownerUserID
        self.policyRevision = policyRevision
        self.agentEnabled = agentEnabled
        self.mcps = mcps
        self.plugins = plugins
    }

    enum CodingKeys: String, CodingKey {
        case agentKey = "agent_key"
        case ownerUserID = "owner_user_id"
        case policyRevision = "policy_revision"
        case agentEnabled = "agent_enabled"
        case mcps, plugins
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        agentKey = try container.decode(String.self, forKey: .agentKey)
        ownerUserID = try container.decode(String.self, forKey: .ownerUserID)
        policyRevision = try container.decode(String.self, forKey: .policyRevision)
        agentEnabled = try container.decodeIfPresent(Bool.self, forKey: .agentEnabled) ?? true
        mcps = try container.decodeIfPresent([GatewayResolvedMCPDTO].self, forKey: .mcps) ?? []
        plugins = try container.decodeIfPresent([GatewayResolvedPluginDTO].self, forKey: .plugins) ?? []
    }
}

struct GatewayResolvedMCPDTO: Decodable, Sendable {
    var resource: GatewayMCPResourceDTO
    var binding: GatewayCapabilityBindingDTO
    var available: Bool
    var status: String
    var toolSnapshot: [LocalAgentJSONValue]

    enum CodingKeys: String, CodingKey {
        case resource, binding, available, status
        case toolSnapshot = "tool_snapshot"
    }
}

struct GatewayMCPResourceDTO: Decodable, Sendable {
    var id: String
    var name: String
    var displayName: String
    var description: String?
    var enabled: Bool
    var runtime: GatewayMCPRuntimeDTO

    enum CodingKeys: String, CodingKey {
        case id, name, description, enabled, runtime
        case displayName = "display_name"
    }
}

struct GatewayMCPRuntimeDTO: Decodable, Sendable {
    var kind: String
    var builtinKind: String?
    var systemKey: String?
    var serverName: String?
    var url: String?
    var headers: [String: String]

    enum CodingKeys: String, CodingKey {
        case kind, url, headers
        case builtinKind = "builtin_kind"
        case systemKey = "system_key"
        case serverName = "server_name"
    }

    /// Plugin Management persists unified system MCPs with `system_key` and intentionally
    /// leaves the legacy `builtin_kind` field empty. Local Agent Host still freezes the
    /// corresponding capability by its canonical BuiltinMcpKind name.
    var resolvedBuiltinKind: String? {
        if let builtinKind = Self.normalized(builtinKind) { return builtinKind }
        let key = Self.normalized(systemKey) ?? Self.normalized(serverName)
        switch key?.lowercased() {
        case "code_maintainer_read": return "CodeMaintainerRead"
        case "code_maintainer_write": return "CodeMaintainerWrite"
        case "terminal_controller": return "TerminalController"
        case "notepad": return "Notepad"
        case "agent_builder": return "AgentBuilder"
        case "ask_user": return "AskUser"
        case "remote_connection_controller": return "RemoteConnectionController"
        case "memory_skill_reader": return "MemorySkillReader"
        case "memory_command_reader": return "MemoryCommandReader"
        case "memory_plugin_reader": return "MemoryPluginReader"
        default: return nil
        }
    }

    var isExternalHTTP: Bool {
        kind.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() == "http"
            && Self.normalized(url) != nil
    }

    private static func normalized(_ value: String?) -> String? {
        let normalized = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return normalized.isEmpty ? nil : normalized
    }
}

struct GatewayCapabilityBindingDTO: Decodable, Sendable {
    var enabled: Bool
    var required: Bool
}

struct GatewayResolvedPluginDTO: Decodable, Sendable {
    var catalog: GatewayPluginCapabilityCatalogDTO
    var binding: GatewayCapabilityBindingDTO
    var available: Bool
    var status: String
}

struct GatewayPluginCapabilityCatalogDTO: Decodable, Sendable {
    var id: String
    var pluginKey: String
    var displayName: String
    var description: String

    enum CodingKeys: String, CodingKey {
        case id, description
        case pluginKey = "plugin_key"
        case displayName = "display_name"
    }
}

struct GatewayPluginSourceListDTO: Decodable, Sendable {
    var items: [GatewayPluginSourceDTO]
}

struct GatewayPluginSourceDTO: Decodable, Sendable {
    var catalog: GatewayPluginCatalogDTO
    var release: GatewayPluginReleaseDTO
    var preference: GatewayPluginPreferenceDTO?
}

struct GatewayPluginCatalogDTO: Decodable, Sendable {
    var id: String
    var displayName: String?
    var name: String?
    var description: String?
    var publisher: GatewayPluginPublisherDTO?
    var interface: GatewayPluginInterfaceDTO?
    var pluginKey: String? = nil
    var hasUI: Bool? = nil
    enum CodingKeys: String, CodingKey {
        case id, name, description, publisher, interface
        case pluginKey = "plugin_key"
        case displayName = "display_name"
        case hasUI = "has_ui"
    }
}

struct GatewayPluginPublisherDTO: Decodable, Sendable {
    var id: String?
    var name: String?
}

struct GatewayPluginInterfaceDTO: Decodable, Sendable {
    var category: String?
    var developerName: String?
}

struct GatewayPluginReleaseDTO: Decodable, Sendable {
    var id: String
    var version: String?
    var artifactSHA256: String?
    var npmPackage: GatewayPluginNPMPackageDTO?

    enum CodingKeys: String, CodingKey {
        case id, version
        case artifactSHA256 = "artifact_sha256"
        case npmPackage = "npm_package"
    }
}

struct GatewayPluginNPMPackageDTO: Decodable, Sendable {
    var name: String
    var version: String
    var integrity: String
}

struct GatewayPluginPreferenceDTO: Decodable, Sendable {
    var enabled: Bool
}

struct GatewayPluginPreferenceRequest: Encodable {
    var deviceID: String
    var enabled: Bool
    enum CodingKeys: String, CodingKey {
        case enabled
        case deviceID = "device_id"
    }
}

struct GatewayPluginPreferenceResponse: Decodable, Sendable {
    var enabled: Bool?
}
