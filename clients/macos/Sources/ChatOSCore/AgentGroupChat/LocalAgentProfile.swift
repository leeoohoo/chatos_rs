import Foundation

public enum LocalAgentProfileStatus: String, Codable, Sendable {
    case active, archived
}

public struct LocalAgentProfileDraft: Codable, Sendable, Equatable {
    public let name: String
    public let description: String
    public let rolePrompt: String
    public let modelConfigID: String
    public let thinkingLevel: String?
    public let professionKey: String
    public let defaultPluginIDs: [String]
    public let defaultSkillIDs: [String]
    public let heartbeatEnabled: Bool
    public let heartbeatIntervalSeconds: Int
    public let heartbeatPrompt: String

    public init(
        name: String,
        description: String = "",
        rolePrompt: String,
        modelConfigID: String,
        thinkingLevel: String? = nil,
        professionKey: String = LocalAgentSkillCatalog.legacyProfessionKey,
        defaultPluginIDs: [String] = [],
        defaultSkillIDs: [String] = [],
        heartbeatEnabled: Bool = false,
        heartbeatIntervalSeconds: Int = 900,
        heartbeatPrompt: String = ""
    ) {
        self.name = name
        self.description = description
        self.rolePrompt = rolePrompt
        self.modelConfigID = modelConfigID
        self.thinkingLevel = thinkingLevel?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
        self.professionKey = professionKey
        self.defaultPluginIDs = defaultPluginIDs
        self.defaultSkillIDs = defaultSkillIDs
        self.heartbeatEnabled = heartbeatEnabled
        self.heartbeatIntervalSeconds = heartbeatIntervalSeconds
        self.heartbeatPrompt = heartbeatPrompt
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(name, field: "name", maximumLength: 120)
        try AgentGroupChatValidation.optionalText(description, field: "description", maximumLength: 2_000)
        try AgentGroupChatValidation.text(rolePrompt, field: "rolePrompt", maximumLength: 32_000)
        try AgentGroupChatValidation.identifier(modelConfigID, field: "modelConfigID")
        if let thinkingLevel {
            guard LocalAgentThinkingLevelCatalog.allValues.contains(thinkingLevel) else {
                throw AgentGroupChatError.invalidField("thinkingLevel")
            }
        }
        _ = try LocalAgentSkillCatalog.requireProfession(key: professionKey)
        try AgentGroupChatValidation.identifiers(defaultPluginIDs, field: "defaultPluginIDs", maximumCount: 100)
        try AgentGroupChatValidation.identifiers(defaultSkillIDs, field: "defaultSkillIDs", maximumCount: 100)
        guard (60...86_400).contains(heartbeatIntervalSeconds) else {
            throw AgentGroupChatError.invalidField("heartbeatIntervalSeconds")
        }
        try AgentGroupChatValidation.optionalText(
            heartbeatPrompt,
            field: "heartbeatPrompt",
            maximumLength: 8_000
        )
    }

    private enum CodingKeys: String, CodingKey {
        case name, description, rolePrompt, modelConfigID, thinkingLevel, professionKey
        case defaultPluginIDs, defaultSkillIDs
        case heartbeatEnabled, heartbeatIntervalSeconds, heartbeatPrompt
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            name: try values.decode(String.self, forKey: .name),
            description: try values.decodeIfPresent(String.self, forKey: .description) ?? "",
            rolePrompt: try values.decode(String.self, forKey: .rolePrompt),
            modelConfigID: try values.decode(String.self, forKey: .modelConfigID),
            thinkingLevel: try values.decodeIfPresent(String.self, forKey: .thinkingLevel),
            professionKey: try values.decodeIfPresent(String.self, forKey: .professionKey)
                ?? LocalAgentSkillCatalog.legacyProfessionKey,
            defaultPluginIDs: try values.decodeIfPresent([String].self, forKey: .defaultPluginIDs) ?? [],
            defaultSkillIDs: try values.decodeIfPresent([String].self, forKey: .defaultSkillIDs) ?? [],
            heartbeatEnabled: try values.decodeIfPresent(Bool.self, forKey: .heartbeatEnabled) ?? false,
            heartbeatIntervalSeconds: try values.decodeIfPresent(
                Int.self,
                forKey: .heartbeatIntervalSeconds
            ) ?? 900,
            heartbeatPrompt: try values.decodeIfPresent(String.self, forKey: .heartbeatPrompt) ?? ""
        )
    }
}

/// A user-reviewable proposal produced by the built-in Agent Builder. It deliberately contains
/// only profile and room-member fields: account, project, room and creation authority stay in the
/// host application and can never be selected by model tool arguments.
public struct LocalAgentDraft: Codable, Sendable, Equatable {
    public let name: String
    public let role: String
    public let responsibility: String
    public let rolePrompt: String
    public let modelConfigID: String
    public let thinkingLevel: String?
    public let professionKey: String
    public let rationale: String

    public init(
        name: String,
        role: String,
        responsibility: String = "",
        rolePrompt: String,
        modelConfigID: String,
        thinkingLevel: String? = nil,
        professionKey: String = LocalAgentSkillCatalog.legacyProfessionKey,
        rationale: String = ""
    ) {
        self.name = name
        self.role = role
        self.responsibility = responsibility
        self.rolePrompt = rolePrompt
        self.modelConfigID = modelConfigID
        self.thinkingLevel = thinkingLevel?
            .trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
        self.professionKey = professionKey
        self.rationale = rationale
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(name, field: "name", maximumLength: 120)
        try AgentGroupChatValidation.text(role, field: "role", maximumLength: 160)
        try AgentGroupChatValidation.optionalText(
            responsibility,
            field: "responsibility",
            maximumLength: 8_000
        )
        try AgentGroupChatValidation.text(rolePrompt, field: "rolePrompt", maximumLength: 32_000)
        try AgentGroupChatValidation.identifier(modelConfigID, field: "modelConfigID")
        if let thinkingLevel {
            guard LocalAgentThinkingLevelCatalog.allValues.contains(thinkingLevel) else {
                throw AgentGroupChatError.invalidField("thinkingLevel")
            }
        }
        _ = try LocalAgentSkillCatalog.requireProfession(key: professionKey)
        try AgentGroupChatValidation.optionalText(rationale, field: "rationale", maximumLength: 4_000)
    }

    private enum CodingKeys: String, CodingKey {
        case name, role, responsibility, rolePrompt, modelConfigID, thinkingLevel
        case professionKey, rationale
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            name: try values.decode(String.self, forKey: .name),
            role: try values.decode(String.self, forKey: .role),
            responsibility: try values.decodeIfPresent(String.self, forKey: .responsibility) ?? "",
            rolePrompt: try values.decode(String.self, forKey: .rolePrompt),
            modelConfigID: try values.decode(String.self, forKey: .modelConfigID),
            thinkingLevel: try values.decodeIfPresent(String.self, forKey: .thinkingLevel),
            professionKey: try values.decodeIfPresent(String.self, forKey: .professionKey)
                ?? LocalAgentSkillCatalog.legacyProfessionKey,
            rationale: try values.decodeIfPresent(String.self, forKey: .rationale) ?? ""
        )
    }

    public var profileDraft: LocalAgentProfileDraft {
        .init(
            name: name,
            description: responsibility,
            rolePrompt: rolePrompt,
            modelConfigID: modelConfigID,
            thinkingLevel: thinkingLevel,
            professionKey: professionKey
        )
    }

    public var memberDraft: ProjectAgentRoomMemberDraft {
        .init(role: role, responsibility: responsibility)
    }
}

public struct LocalAgentBuilderModelOption: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let provider: String
    public let modelName: String
    public let supportsReasoning: Bool
    public let defaultThinkingLevel: String?
    public let thinkingLevels: [String]

    public init(
        id: String,
        name: String,
        provider: String,
        modelName: String,
        supportsReasoning: Bool = false,
        defaultThinkingLevel: String? = nil,
        thinkingLevels: [String] = []
    ) {
        self.id = id
        self.name = name
        self.provider = provider
        self.modelName = modelName
        self.supportsReasoning = supportsReasoning
        self.defaultThinkingLevel = defaultThinkingLevel
        self.thinkingLevels = thinkingLevels
    }

    private enum CodingKeys: String, CodingKey {
        case id, name, provider, modelName, supportsReasoning, defaultThinkingLevel, thinkingLevels
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            id: try values.decode(String.self, forKey: .id),
            name: try values.decode(String.self, forKey: .name),
            provider: try values.decode(String.self, forKey: .provider),
            modelName: try values.decode(String.self, forKey: .modelName),
            supportsReasoning: try values.decodeIfPresent(Bool.self, forKey: .supportsReasoning) ?? false,
            defaultThinkingLevel: try values.decodeIfPresent(String.self, forKey: .defaultThinkingLevel),
            thinkingLevels: try values.decodeIfPresent([String].self, forKey: .thinkingLevels) ?? []
        )
    }
}

public enum LocalAgentThinkingLevelCatalog {
    public static let allValues = Set([
        "auto", "none", "minimal", "low", "medium", "high", "xhigh", "max",
    ])

    public static func values(provider: String?) -> [String] {
        switch normalizedProvider(provider) {
        case "deepseek": ["none", "high", "max"]
        case "kimi", "kimik2", "moonshot": ["auto", "none"]
        case "glm", "zhipu", "zai": ["none", "low", "medium", "high", "xhigh"]
        default: ["none", "minimal", "low", "medium", "high", "xhigh"]
        }
    }

    public static func normalized(_ value: String?, allowedValues: [String]) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty,
              allowedValues.contains(value) else { return nil }
        return value
    }

    private static func normalizedProvider(_ provider: String?) -> String {
        (provider ?? "gpt")
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
            .replacingOccurrences(of: "-", with: "_")
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}

public struct LocalAgentBuilderPluginOption: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let description: String

    public init(id: String, name: String, description: String) {
        self.id = id
        self.name = name
        self.description = description
    }
}

public struct LocalAgentBuilderResources: Codable, Sendable, Equatable {
    public let models: [LocalAgentBuilderModelOption]
    public let plugins: [LocalAgentBuilderPluginOption]

    public init(
        models: [LocalAgentBuilderModelOption],
        plugins: [LocalAgentBuilderPluginOption]
    ) {
        self.models = models
        self.plugins = plugins
    }
}

public struct LocalAgentProfile: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let draft: LocalAgentProfileDraft
    public let status: LocalAgentProfileStatus
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64
    public let lastHeartbeatAtUnixMs: Int64?
    public let nextHeartbeatAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        draft: LocalAgentProfileDraft,
        status: LocalAgentProfileStatus = .active,
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64,
        lastHeartbeatAtUnixMs: Int64? = nil,
        nextHeartbeatAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.status = status
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
        self.lastHeartbeatAtUnixMs = lastHeartbeatAtUnixMs
        self.nextHeartbeatAtUnixMs = nextHeartbeatAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "id")
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try draft.validate()
        try AgentGroupChatValidation.timestamps(createdAtUnixMs, updatedAtUnixMs)
        guard lastHeartbeatAtUnixMs == nil || lastHeartbeatAtUnixMs! >= 0,
              nextHeartbeatAtUnixMs == nil || nextHeartbeatAtUnixMs! >= 0 else {
            throw AgentGroupChatError.invalidField("heartbeatTimestamp")
        }
    }
}
