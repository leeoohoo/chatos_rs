import Foundation

public enum AgentGroupChatError: Error, Equatable, LocalizedError {
    case invalidField(String)
    case notFound
    case conflict
    case notMember
    case permissionDenied
    case storage(String)

    public var errorDescription: String? {
        switch self {
        case let .invalidField(field): "无效的 Agent 群聊字段：\(field)"
        case .notFound: "Agent 群聊资源不存在。"
        case .conflict: "Agent 群聊状态已经变化，请刷新后重试。"
        case .notMember: "Agent 不是当前群聊成员。"
        case .permissionDenied: "当前身份不能执行这个群聊操作。"
        case let .storage(message): "本地 Agent 群聊存储不可用：\(message)"
        }
    }
}

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

    public init(
        name: String,
        description: String = "",
        rolePrompt: String,
        modelConfigID: String,
        thinkingLevel: String? = nil,
        professionKey: String = LocalAgentSkillCatalog.legacyProfessionKey,
        defaultPluginIDs: [String] = [],
        defaultSkillIDs: [String] = []
    ) {
        self.name = name
        self.description = description
        self.rolePrompt = rolePrompt
        self.modelConfigID = modelConfigID
        self.thinkingLevel = thinkingLevel?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
        self.professionKey = professionKey
        self.defaultPluginIDs = defaultPluginIDs
        self.defaultSkillIDs = defaultSkillIDs
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
    }

    private enum CodingKeys: String, CodingKey {
        case name, description, rolePrompt, modelConfigID, thinkingLevel, professionKey
        case defaultPluginIDs, defaultSkillIDs
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
            defaultSkillIDs: try values.decodeIfPresent([String].self, forKey: .defaultSkillIDs) ?? []
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
    public let professionKey: String
    public let rationale: String

    public init(
        name: String,
        role: String,
        responsibility: String = "",
        rolePrompt: String,
        modelConfigID: String,
        professionKey: String = LocalAgentSkillCatalog.legacyProfessionKey,
        rationale: String = ""
    ) {
        self.name = name
        self.role = role
        self.responsibility = responsibility
        self.rolePrompt = rolePrompt
        self.modelConfigID = modelConfigID
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
        _ = try LocalAgentSkillCatalog.requireProfession(key: professionKey)
        try AgentGroupChatValidation.optionalText(rationale, field: "rationale", maximumLength: 4_000)
    }

    private enum CodingKeys: String, CodingKey {
        case name, role, responsibility, rolePrompt, modelConfigID, professionKey, rationale
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            name: try values.decode(String.self, forKey: .name),
            role: try values.decode(String.self, forKey: .role),
            responsibility: try values.decodeIfPresent(String.self, forKey: .responsibility) ?? "",
            rolePrompt: try values.decode(String.self, forKey: .rolePrompt),
            modelConfigID: try values.decode(String.self, forKey: .modelConfigID),
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
            professionKey: professionKey
        )
    }

    public var memberDraft: ProjectAgentRoomMemberDraft {
        .init(role: role, responsibility: responsibility)
    }
}

public enum LocalAgentCreationProposalStatus: String, Codable, Sendable {
    case pending, approved, rejected
}

/// Ordinary Agents can request a new teammate through Relay, but only the Human can resolve the
/// proposal. Account, project and room authority are supplied by the identity-bound MCP session.
public struct LocalAgentCreationProposal: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let roomID: String
    public let proposerAgentID: String
    public let sourceDeliveryID: String
    public let requestKey: String
    public let draft: LocalAgentDraft
    public let status: LocalAgentCreationProposalStatus
    public let createdAgentID: String?
    public let createdAtUnixMs: Int64
    public let resolvedAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentDraft,
        status: LocalAgentCreationProposalStatus = .pending,
        createdAgentID: String? = nil,
        createdAtUnixMs: Int64,
        resolvedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.proposerAgentID = proposerAgentID
        self.sourceDeliveryID = sourceDeliveryID
        self.requestKey = requestKey
        self.draft = draft
        self.status = status
        self.createdAgentID = createdAgentID
        self.createdAtUnixMs = createdAtUnixMs
        self.resolvedAtUnixMs = resolvedAtUnixMs
    }

    public func validate() throws {
        for (value, field) in [
            (id, "id"), (ownerUserID, "ownerUserID"), (roomID, "roomID"),
            (proposerAgentID, "proposerAgentID"), (sourceDeliveryID, "sourceDeliveryID"),
            (requestKey, "requestKey"),
        ] {
            try AgentGroupChatValidation.identifier(value, field: field)
        }
        try draft.validate()
        if let createdAgentID {
            try AgentGroupChatValidation.identifier(createdAgentID, field: "createdAgentID")
        }
        guard createdAtUnixMs >= 0,
              resolvedAtUnixMs == nil || resolvedAtUnixMs! >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("proposalTimestamps")
        }
    }
}

public struct LocalAgentProposalApproval: Codable, Sendable, Equatable {
    public let proposal: LocalAgentCreationProposal
    public let agent: LocalAgentProfile
    public let member: ProjectAgentRoomMember?

    public init(
        proposal: LocalAgentCreationProposal,
        agent: LocalAgentProfile,
        member: ProjectAgentRoomMember? = nil
    ) {
        self.proposal = proposal
        self.agent = agent
        self.member = member
    }
}

/// High-risk capabilities are explicit Agent permissions, not Agent types. The values mirror
/// Relay's staffing permission names so prompts, MCP authorization and the management UI share
/// one vocabulary.
public enum LocalAgentPermission {
    public static let staffHire = "agent.staff.hire"
    public static let staffTerminate = "agent.staff.terminate"
    public static let localProjectList = "local.project.list"

    /// Profiles created by the first 3.0.3 preview used this role-like capability. Keep it only
    /// as a read-time compatibility marker; the editor normalizes it into explicit permissions.
    public static let legacyProjectSteward = "builtin.project-steward"

    public static func canManageStaff(_ permissions: [String]) -> Bool {
        let values = Set(permissions)
        return values.contains(legacyProjectSteward)
            || (values.contains(staffHire) && values.contains(staffTerminate))
    }

    public static func canAccessLocalProjects(_ permissions: [String]) -> Bool {
        let values = Set(permissions)
        return values.contains(legacyProjectSteward) || values.contains(localProjectList)
    }

    public static func normalized(
        preserving permissions: [String],
        canManageStaff: Bool,
        canAccessLocalProjects: Bool = false
    ) -> [String] {
        var values = Set(permissions)
        values.remove(legacyProjectSteward)
        values.remove(staffHire)
        values.remove(staffTerminate)
        values.remove(localProjectList)
        if canManageStaff {
            values.insert(staffHire)
            values.insert(staffTerminate)
        }
        if canAccessLocalProjects { values.insert(localProjectList) }
        return values.sorted()
    }
}

public enum LocalAgentRemovalProposalStatus: String, Codable, Sendable {
    case pending, approved, rejected
}

/// Removing a teammate is scoped to the current project team. The reusable Agent profile and its
/// private Memory remain intact; only a Human-approved proposal can change membership.
public struct LocalAgentRemovalProposalDraft: Codable, Sendable, Equatable {
    public let targetAgentID: String
    public let reason: String
    public let handoffPlan: String

    public init(targetAgentID: String, reason: String, handoffPlan: String = "") {
        self.targetAgentID = targetAgentID
        self.reason = reason
        self.handoffPlan = handoffPlan
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(targetAgentID, field: "targetAgentID")
        try AgentGroupChatValidation.text(reason, field: "reason", maximumLength: 4_000)
        try AgentGroupChatValidation.optionalText(
            handoffPlan,
            field: "handoffPlan",
            maximumLength: 8_000
        )
    }
}

public struct LocalAgentRemovalProposal: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let roomID: String
    public let proposerAgentID: String
    public let sourceDeliveryID: String
    public let requestKey: String
    public let draft: LocalAgentRemovalProposalDraft
    public let status: LocalAgentRemovalProposalStatus
    public let createdAtUnixMs: Int64
    public let resolvedAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentRemovalProposalDraft,
        status: LocalAgentRemovalProposalStatus = .pending,
        createdAtUnixMs: Int64,
        resolvedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.proposerAgentID = proposerAgentID
        self.sourceDeliveryID = sourceDeliveryID
        self.requestKey = requestKey
        self.draft = draft
        self.status = status
        self.createdAtUnixMs = createdAtUnixMs
        self.resolvedAtUnixMs = resolvedAtUnixMs
    }

    public func validate() throws {
        for (value, field) in [
            (id, "id"), (ownerUserID, "ownerUserID"), (roomID, "roomID"),
            (proposerAgentID, "proposerAgentID"), (sourceDeliveryID, "sourceDeliveryID"),
            (requestKey, "requestKey"),
        ] {
            try AgentGroupChatValidation.identifier(value, field: field)
        }
        try draft.validate()
        guard createdAtUnixMs >= 0,
              resolvedAtUnixMs == nil || resolvedAtUnixMs! >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("removalProposalTimestamps")
        }
    }
}

public enum LocalAgentTeamCreationProposalStatus: String, Codable, Sendable {
    case pending, approved, rejected
}

/// The project identifier is resolved from a run-scoped opaque option entirely inside the client,
/// then copied verbatim into this durable proposal. It never enters model input or output. Project
/// names remain display metadata and are never used to resolve the target project.
public struct LocalAgentTeamCreationProposalDraft: Codable, Sendable, Equatable {
    public let existingProjectID: String?
    public let newProjectName: String?
    public let newProjectDescription: String
    public let newProjectTypeKey: String?
    public let teamName: String
    public let teamGoal: String

    public init(
        existingProjectID: String,
        teamName: String,
        teamGoal: String = ""
    ) {
        self.existingProjectID = existingProjectID
        newProjectName = nil
        newProjectDescription = ""
        newProjectTypeKey = nil
        self.teamName = teamName
        self.teamGoal = teamGoal
    }

    public init(
        newProjectName: String,
        newProjectDescription: String = "",
        newProjectTypeKey: String = LocalAgentSkillCatalog.legacyProjectTypeKey,
        teamName: String,
        teamGoal: String = ""
    ) {
        existingProjectID = nil
        self.newProjectName = newProjectName
        self.newProjectDescription = newProjectDescription
        self.newProjectTypeKey = newProjectTypeKey
        self.teamName = teamName
        self.teamGoal = teamGoal
    }

    public func validate() throws {
        guard (existingProjectID == nil) != (newProjectName == nil) else {
            throw AgentGroupChatError.invalidField("projectSelection")
        }
        if let existingProjectID {
            try AgentGroupChatValidation.identifier(existingProjectID, field: "projectID")
            guard newProjectDescription.isEmpty else {
                throw AgentGroupChatError.invalidField("newProjectDescription")
            }
            guard newProjectTypeKey == nil else {
                throw AgentGroupChatError.invalidField("newProjectTypeKey")
            }
        }
        if let newProjectName {
            try AgentGroupChatValidation.text(
                newProjectName,
                field: "newProjectName",
                maximumLength: 160
            )
            try AgentGroupChatValidation.optionalText(
                newProjectDescription,
                field: "newProjectDescription",
                maximumLength: 8_000
            )
            if let newProjectTypeKey {
                do {
                    _ = try LocalAgentSkillCatalog.requireProjectType(key: newProjectTypeKey)
                } catch {
                    throw AgentGroupChatError.invalidField("newProjectTypeKey")
                }
            }
        }
        try AgentGroupChatValidation.text(teamName, field: "teamName", maximumLength: 160)
        try AgentGroupChatValidation.optionalText(teamGoal, field: "teamGoal", maximumLength: 8_000)
    }
}

public struct LocalAgentTeamCreationProposal: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let sourceRoomID: String
    public let proposerAgentID: String
    public let sourceDeliveryID: String
    public let requestKey: String
    public let draft: LocalAgentTeamCreationProposalDraft
    public let status: LocalAgentTeamCreationProposalStatus
    public let createdRoomID: String?
    public let createdAtUnixMs: Int64
    public let resolvedAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentTeamCreationProposalDraft,
        status: LocalAgentTeamCreationProposalStatus = .pending,
        createdRoomID: String? = nil,
        createdAtUnixMs: Int64,
        resolvedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.sourceRoomID = sourceRoomID
        self.proposerAgentID = proposerAgentID
        self.sourceDeliveryID = sourceDeliveryID
        self.requestKey = requestKey
        self.draft = draft
        self.status = status
        self.createdRoomID = createdRoomID
        self.createdAtUnixMs = createdAtUnixMs
        self.resolvedAtUnixMs = resolvedAtUnixMs
    }

    public func validate() throws {
        for (value, field) in [
            (id, "id"), (ownerUserID, "ownerUserID"), (sourceRoomID, "sourceRoomID"),
            (proposerAgentID, "proposerAgentID"), (sourceDeliveryID, "sourceDeliveryID"),
            (requestKey, "requestKey"),
        ] {
            try AgentGroupChatValidation.identifier(value, field: field)
        }
        try draft.validate()
        if let createdRoomID {
            try AgentGroupChatValidation.identifier(createdRoomID, field: "createdRoomID")
        }
        guard createdAtUnixMs >= 0,
              resolvedAtUnixMs == nil || resolvedAtUnixMs! >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("teamProposalTimestamps")
        }
    }
}

public struct LocalAgentTeamProposalApproval: Codable, Sendable, Equatable {
    public let proposal: LocalAgentTeamCreationProposal
    public let room: ProjectAgentRoom

    public init(proposal: LocalAgentTeamCreationProposal, room: ProjectAgentRoom) {
        self.proposal = proposal
        self.room = room
    }
}

public enum LocalProjectCreationProposalStatus: String, Codable, Sendable {
    case pending, approved, rejected
}

/// A project proposal contains only host-independent metadata. The Agent cannot choose or infer
/// an absolute directory; the Human selects an authorized local workspace when confirming it.
public struct LocalProjectCreationProposalDraft: Codable, Sendable, Equatable {
    public let name: String
    public let description: String
    public let projectTypeKey: String

    public init(
        name: String,
        description: String = "",
        projectTypeKey: String = LocalAgentSkillCatalog.legacyProjectTypeKey
    ) {
        self.name = name
        self.description = description
        self.projectTypeKey = projectTypeKey
    }

    public func validate() throws {
        try ProjectRegistryValidation.identifier(name, field: "projectProposal.name")
        guard name.count <= 160,
              description.count <= 8_000,
              !description.contains("\0") else {
            throw AgentGroupChatError.invalidField("projectProposal")
        }
        do {
            _ = try LocalAgentSkillCatalog.requireProjectType(key: projectTypeKey)
        } catch {
            throw AgentGroupChatError.invalidField("projectTypeKey")
        }
    }

    private enum CodingKeys: String, CodingKey { case name, description, projectTypeKey }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            name: try values.decode(String.self, forKey: .name),
            description: try values.decodeIfPresent(String.self, forKey: .description) ?? "",
            projectTypeKey: try values.decodeIfPresent(String.self, forKey: .projectTypeKey)
                ?? LocalAgentSkillCatalog.legacyProjectTypeKey
        )
    }
}

public struct LocalProjectCreationProposal: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let roomID: String
    public let proposerAgentID: String
    public let sourceDeliveryID: String
    public let requestKey: String
    public let draft: LocalProjectCreationProposalDraft
    public let status: LocalProjectCreationProposalStatus
    public let createdProjectID: String?
    public let createdAtUnixMs: Int64
    public let resolvedAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalProjectCreationProposalDraft,
        status: LocalProjectCreationProposalStatus = .pending,
        createdProjectID: String? = nil,
        createdAtUnixMs: Int64,
        resolvedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.proposerAgentID = proposerAgentID
        self.sourceDeliveryID = sourceDeliveryID
        self.requestKey = requestKey
        self.draft = draft
        self.status = status
        self.createdProjectID = createdProjectID
        self.createdAtUnixMs = createdAtUnixMs
        self.resolvedAtUnixMs = resolvedAtUnixMs
    }

    public func validate() throws {
        for (value, field) in [
            (id, "id"), (ownerUserID, "ownerUserID"), (roomID, "roomID"),
            (proposerAgentID, "proposerAgentID"), (sourceDeliveryID, "sourceDeliveryID"),
            (requestKey, "requestKey"),
        ] {
            try AgentGroupChatValidation.identifier(value, field: field)
        }
        try draft.validate()
        if let createdProjectID {
            try AgentGroupChatValidation.identifier(createdProjectID, field: "createdProjectID")
        }
        guard createdAtUnixMs >= 0,
              resolvedAtUnixMs == nil || resolvedAtUnixMs! >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("projectProposalTimestamps")
        }
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

    public init(
        id: String,
        ownerUserID: String,
        draft: LocalAgentProfileDraft,
        status: LocalAgentProfileStatus = .active,
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.status = status
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "id")
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try draft.validate()
        try AgentGroupChatValidation.timestamps(createdAtUnixMs, updatedAtUnixMs)
    }
}

public enum ProjectAgentRoomStatus: String, Codable, Sendable {
    case active, archived
}

/// Project teams and private conversations share the same durable transcript, unread cursor,
/// delivery queue and Relay MCP. The kind only controls participants, routing and project access.
public enum LocalAgentConversationKind: String, Codable, Sendable {
    case projectTeam = "project_team"
    case humanAgentDirect = "human_agent_direct"
    case agentAgentDirect = "agent_agent_direct"

    public var isDirect: Bool { self != .projectTeam }
}

public struct ProjectAgentRoomDraft: Codable, Sendable, Equatable {
    public let name: String
    public let goal: String

    public init(name: String, goal: String = "") {
        self.name = name
        self.goal = goal
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(name, field: "name", maximumLength: 160)
        try AgentGroupChatValidation.optionalText(goal, field: "goal", maximumLength: 8_000)
    }
}

public struct ProjectAgentRoom: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let projectID: String
    public let draft: ProjectAgentRoomDraft
    public let defaultAgentID: String?
    public let conversationKind: LocalAgentConversationKind
    public let directKey: String?
    public let status: ProjectAgentRoomStatus
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    public init(
        id: String,
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft,
        defaultAgentID: String? = nil,
        conversationKind: LocalAgentConversationKind = .projectTeam,
        directKey: String? = nil,
        status: ProjectAgentRoomStatus = .active,
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.draft = draft
        self.defaultAgentID = defaultAgentID
        self.conversationKind = conversationKind
        self.directKey = directKey
        self.status = status
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "id")
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try draft.validate()
        if let defaultAgentID {
            try AgentGroupChatValidation.identifier(defaultAgentID, field: "defaultAgentID")
        }
        switch conversationKind {
        case .projectTeam:
            guard directKey == nil else {
                throw AgentGroupChatError.invalidField("directKey")
            }
        case .humanAgentDirect, .agentAgentDirect:
            guard let directKey else {
                throw AgentGroupChatError.invalidField("directKey")
            }
            try AgentGroupChatValidation.identifier(directKey, field: "directKey")
        }
        try AgentGroupChatValidation.timestamps(createdAtUnixMs, updatedAtUnixMs)
    }
}

public enum ProjectAgentRoomMemberStatus: String, Codable, Sendable {
    case active, removed
}

public struct ProjectAgentRoomMemberDraft: Codable, Sendable, Equatable {
    public let role: String
    public let responsibility: String
    public let pluginAllowlist: [String]

    public init(role: String, responsibility: String = "", pluginAllowlist: [String] = []) {
        self.role = role
        self.responsibility = responsibility
        self.pluginAllowlist = pluginAllowlist
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(role, field: "role", maximumLength: 160)
        try AgentGroupChatValidation.optionalText(
            responsibility,
            field: "responsibility",
            maximumLength: 8_000
        )
        try AgentGroupChatValidation.identifiers(
            pluginAllowlist,
            field: "pluginAllowlist",
            maximumCount: 100
        )
    }
}

public struct ProjectAgentRoomMember: Codable, Sendable, Equatable, Identifiable {
    public var id: String { "\(roomID):\(agentID)" }
    public let ownerUserID: String
    public let roomID: String
    public let agentID: String
    public let draft: ProjectAgentRoomMemberDraft
    public let status: ProjectAgentRoomMemberStatus
    public let joinedAtUnixMs: Int64

    public init(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        draft: ProjectAgentRoomMemberDraft,
        status: ProjectAgentRoomMemberStatus = .active,
        joinedAtUnixMs: Int64
    ) {
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.agentID = agentID
        self.draft = draft
        self.status = status
        self.joinedAtUnixMs = joinedAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try draft.validate()
        guard joinedAtUnixMs >= 0 else { throw AgentGroupChatError.invalidField("joinedAtUnixMs") }
    }
}

public struct LocalAgentMembershipUpdateResult: Codable, Sendable, Equatable {
    public let profile: LocalAgentProfile
    public let member: ProjectAgentRoomMember

    public init(profile: LocalAgentProfile, member: ProjectAgentRoomMember) {
        self.profile = profile
        self.member = member
    }
}

public enum ProjectAgentMessageSenderKind: String, Codable, Sendable {
    case human, agent, system
}

public struct ProjectAgentMessageAttachmentDraft: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let mimeType: String
    public let kind: ConversationAttachmentKind
    public let origin: ConversationAttachmentOrigin
    public let data: Data

    public init(
        id: String = UUID().uuidString.lowercased(),
        name: String,
        mimeType: String,
        kind: ConversationAttachmentKind,
        origin: ConversationAttachmentOrigin,
        data: Data
    ) {
        self.id = id
        self.name = name
        self.mimeType = mimeType
        self.kind = kind
        self.origin = origin
        self.data = data
    }

    public init(_ attachment: ConversationAttachmentDraft) {
        self.init(
            id: attachment.id,
            name: attachment.name,
            mimeType: attachment.mimeType,
            kind: attachment.kind,
            origin: attachment.origin,
            data: attachment.data
        )
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "attachment.id")
        try AgentGroupChatValidation.text(name, field: "attachment.name", maximumLength: 512)
        try AgentGroupChatValidation.text(
            mimeType,
            field: "attachment.mimeType",
            maximumLength: 255
        )
        guard !data.isEmpty, data.count <= 20 * 1_024 * 1_024 else {
            throw AgentGroupChatError.invalidField("attachment.data")
        }
    }
}

public struct ProjectAgentMessageAttachment: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let mimeType: String
    public let size: Int
    public let kind: ConversationAttachmentKind
    public let origin: ConversationAttachmentOrigin

    public init(
        id: String,
        name: String,
        mimeType: String,
        size: Int,
        kind: ConversationAttachmentKind,
        origin: ConversationAttachmentOrigin
    ) {
        self.id = id
        self.name = name
        self.mimeType = mimeType
        self.size = size
        self.kind = kind
        self.origin = origin
    }
}

public struct ProjectAgentMessageAttachmentPayload: Sendable, Equatable {
    public let attachment: ProjectAgentMessageAttachment
    public let localFileURL: URL

    public init(attachment: ProjectAgentMessageAttachment, localFileURL: URL) {
        self.attachment = attachment
        self.localFileURL = localFileURL
    }
}

public struct ProjectAgentMessageDraft: Codable, Sendable, Equatable {
    public let senderKind: ProjectAgentMessageSenderKind
    public let senderID: String
    public let content: String
    public let mentionedAgentIDs: [String]
    public let replyToMessageID: String?
    public let sourceRunID: String?
    public let causationID: String?
    public let rootMessageID: String?
    public let hopCount: Int
    public let attachments: [ProjectAgentMessageAttachmentDraft]?

    public init(
        senderKind: ProjectAgentMessageSenderKind,
        senderID: String,
        content: String,
        mentionedAgentIDs: [String] = [],
        replyToMessageID: String? = nil,
        sourceRunID: String? = nil,
        causationID: String? = nil,
        rootMessageID: String? = nil,
        hopCount: Int = 0,
        attachments: [ProjectAgentMessageAttachmentDraft] = []
    ) {
        self.senderKind = senderKind
        self.senderID = senderID
        self.content = content
        self.mentionedAgentIDs = mentionedAgentIDs
        self.replyToMessageID = replyToMessageID
        self.sourceRunID = sourceRunID
        self.causationID = causationID
        self.rootMessageID = rootMessageID
        self.hopCount = hopCount
        self.attachments = attachments.isEmpty ? nil : attachments
    }

    public var attachmentItems: [ProjectAgentMessageAttachmentDraft] { attachments ?? [] }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(senderID, field: "senderID")
        if content.isEmpty, !attachmentItems.isEmpty {
            // An attachment-only Human message is valid and renders without placeholder text.
        } else {
            try AgentGroupChatValidation.text(content, field: "content", maximumLength: 64_000)
        }
        try AgentGroupChatValidation.identifiers(
            mentionedAgentIDs,
            field: "mentionedAgentIDs",
            maximumCount: 32
        )
        for (value, field) in [
            (replyToMessageID, "replyToMessageID"),
            (sourceRunID, "sourceRunID"),
            (causationID, "causationID"),
            (rootMessageID, "rootMessageID"),
        ] where value != nil {
            try AgentGroupChatValidation.identifier(value!, field: field)
        }
        guard (0...64).contains(hopCount) else {
            throw AgentGroupChatError.invalidField("hopCount")
        }
        guard attachmentItems.count <= 20,
              attachmentItems.reduce(0, { $0 + $1.data.count }) <= 20 * 1_024 * 1_024,
              Set(attachmentItems.map(\.id)).count == attachmentItems.count else {
            throw AgentGroupChatError.invalidField("attachments")
        }
        try attachmentItems.forEach { try $0.validate() }
    }
}

public struct ProjectAgentMessage: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let roomID: String
    public let senderKind: ProjectAgentMessageSenderKind
    public let senderID: String
    public let content: String
    public let mentionedAgentIDs: [String]
    public let replyToMessageID: String?
    public let sourceRunID: String?
    public let causationID: String?
    public let rootMessageID: String
    public let hopCount: Int
    public let createdAtUnixMs: Int64
    public let attachments: [ProjectAgentMessageAttachment]?

    public init(
        id: String,
        ownerUserID: String,
        roomID: String,
        draft: ProjectAgentMessageDraft,
        rootMessageID: String,
        attachments: [ProjectAgentMessageAttachment] = [],
        createdAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        senderKind = draft.senderKind
        senderID = draft.senderID
        content = draft.content
        mentionedAgentIDs = draft.mentionedAgentIDs
        replyToMessageID = draft.replyToMessageID
        sourceRunID = draft.sourceRunID
        causationID = draft.causationID
        self.rootMessageID = rootMessageID
        hopCount = draft.hopCount
        self.attachments = attachments.isEmpty ? nil : attachments
        self.createdAtUnixMs = createdAtUnixMs
    }

    public var attachmentItems: [ProjectAgentMessageAttachment] { attachments ?? [] }
}

/// Stable room pagination uses the persisted message identity instead of a timestamp-only cursor.
/// Message ids disambiguate messages written in the same millisecond and are resolved inside the
/// identity-bound room by the store.
public struct ProjectAgentMessagePage: Codable, Sendable, Equatable {
    public let messages: [ProjectAgentMessage]
    public let nextCursorMessageID: String?
    public let hasMore: Bool

    public init(
        messages: [ProjectAgentMessage],
        nextCursorMessageID: String?,
        hasMore: Bool
    ) {
        self.messages = messages
        self.nextCursorMessageID = nextCursorMessageID
        self.hasMore = hasMore
    }
}

/// Each Agent owns an independent read cursor for each room. The cursor is monotonic and cannot
/// be moved backwards by a stale or retried MCP call.
public struct ProjectAgentReadCursor: Codable, Sendable, Equatable {
    public let ownerUserID: String
    public let roomID: String
    public let agentID: String
    public let messageID: String
    public let messageCreatedAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    public init(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        messageID: String,
        messageCreatedAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.agentID = agentID
        self.messageID = messageID
        self.messageCreatedAtUnixMs = messageCreatedAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }
}

public struct ProjectAgentUnreadPage: Codable, Sendable, Equatable {
    public let messages: [ProjectAgentMessage]
    public let nextCursorMessageID: String?
    public let hasMore: Bool
    public let readThroughMessageID: String?

    public init(
        messages: [ProjectAgentMessage],
        nextCursorMessageID: String?,
        hasMore: Bool,
        readThroughMessageID: String?
    ) {
        self.messages = messages
        self.nextCursorMessageID = nextCursorMessageID
        self.hasMore = hasMore
        self.readThroughMessageID = readThroughMessageID
    }
}

public enum ProjectAgentDeliveryTriggerKind: String, Codable, Sendable {
    case mention, defaultAgent = "default_agent", agentMention = "agent_mention"
}

public enum ProjectAgentDeliveryStatus: String, Codable, Sendable {
    case pending, running, completed, failed, cancelled
}

public struct ProjectAgentDelivery: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let roomID: String
    public let messageID: String
    public let rootMessageID: String
    public let targetAgentID: String
    public let triggerKind: ProjectAgentDeliveryTriggerKind
    public let status: ProjectAgentDeliveryStatus
    public let attempt: Int
    public let hopCount: Int
    public let deduplicationKey: String
    public let responseMessageID: String?
    public let lastError: String?
    public let claimedAtUnixMs: Int64?
    public let completedAtUnixMs: Int64?
    public let createdAtUnixMs: Int64

    public init(
        id: String,
        ownerUserID: String,
        roomID: String,
        messageID: String,
        rootMessageID: String,
        targetAgentID: String,
        triggerKind: ProjectAgentDeliveryTriggerKind,
        status: ProjectAgentDeliveryStatus,
        attempt: Int,
        hopCount: Int,
        deduplicationKey: String,
        responseMessageID: String? = nil,
        lastError: String? = nil,
        claimedAtUnixMs: Int64? = nil,
        completedAtUnixMs: Int64? = nil,
        createdAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.messageID = messageID
        self.rootMessageID = rootMessageID
        self.targetAgentID = targetAgentID
        self.triggerKind = triggerKind
        self.status = status
        self.attempt = attempt
        self.hopCount = hopCount
        self.deduplicationKey = deduplicationKey
        self.responseMessageID = responseMessageID
        self.lastError = lastError
        self.claimedAtUnixMs = claimedAtUnixMs
        self.completedAtUnixMs = completedAtUnixMs
        self.createdAtUnixMs = createdAtUnixMs
    }
}

public struct AgentGroupChatRoutingLimits: Codable, Sendable, Equatable {
    public var maximumHopCount: Int
    public var maximumAgentRunsPerRootMessage: Int

    public init(maximumHopCount: Int = 4, maximumAgentRunsPerRootMessage: Int = 12) {
        self.maximumHopCount = maximumHopCount
        self.maximumAgentRunsPerRootMessage = maximumAgentRunsPerRootMessage
    }

    public func validate() throws {
        guard (0...32).contains(maximumHopCount),
              (1...128).contains(maximumAgentRunsPerRootMessage) else {
            throw AgentGroupChatError.invalidField("routingLimits")
        }
    }
}

public struct AgentGroupChatPostResult: Codable, Sendable, Equatable {
    public let message: ProjectAgentMessage
    public let deliveries: [ProjectAgentDelivery]
    public let routingStopReason: String?

    public init(
        message: ProjectAgentMessage,
        deliveries: [ProjectAgentDelivery],
        routingStopReason: String? = nil
    ) {
        self.message = message
        self.deliveries = deliveries
        self.routingStopReason = routingStopReason
    }
}

public protocol AgentGroupChatStore: Sendable {
    func createAgent(ownerUserID: String, draft: LocalAgentProfileDraft) async throws -> LocalAgentProfile
    func listAgents(ownerUserID: String, includeArchived: Bool) async throws -> [LocalAgentProfile]
    func updateAgentProfile(
        ownerUserID: String,
        agentID: String,
        draft: LocalAgentProfileDraft
    ) async throws -> LocalAgentProfile
    func updateAgentMembership(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        profileDraft: LocalAgentProfileDraft,
        memberDraft: ProjectAgentRoomMemberDraft
    ) async throws -> LocalAgentMembershipUpdateResult
    func createAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentCreationProposal
    func listAgentProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalAgentCreationProposalStatus?
    ) async throws -> [LocalAgentCreationProposal]
    func approveAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentProposalApproval
    func rejectAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentCreationProposal
    func createAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentRemovalProposalDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentRemovalProposal
    func listAgentRemovalProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalAgentRemovalProposalStatus?
    ) async throws -> [LocalAgentRemovalProposal]
    func approveAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentRemovalProposal
    func rejectAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentRemovalProposal
    func createTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentTeamCreationProposalDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTeamCreationProposal
    func listTeamProposals(
        ownerUserID: String,
        sourceRoomID: String,
        status: LocalAgentTeamCreationProposalStatus?
    ) async throws -> [LocalAgentTeamCreationProposal]
    func approveTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        resolvedProjectID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTeamProposalApproval
    func rejectTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTeamCreationProposal
    func createProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalProjectCreationProposalDraft,
        nowUnixMs: Int64
    ) async throws -> LocalProjectCreationProposal
    func listProjectProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalProjectCreationProposalStatus?
    ) async throws -> [LocalProjectCreationProposal]
    func approveProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        createdProjectID: String,
        nowUnixMs: Int64
    ) async throws -> LocalProjectCreationProposal
    func rejectProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalProjectCreationProposal
    func createRoom(
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft
    ) async throws -> ProjectAgentRoom
    func openHumanAgentDirect(
        ownerUserID: String,
        agentID: String
    ) async throws -> ProjectAgentRoom
    func openAgentDirect(
        ownerUserID: String,
        initiatingAgentID: String,
        targetAgentID: String
    ) async throws -> ProjectAgentRoom
    func room(ownerUserID: String, roomID: String) async throws -> ProjectAgentRoom?
    func activeRoom(ownerUserID: String, projectID: String) async throws -> ProjectAgentRoom?
    func listRooms(ownerUserID: String, includeArchived: Bool) async throws -> [ProjectAgentRoom]
    func listDirectConversations(
        ownerUserID: String,
        includeArchived: Bool
    ) async throws -> [ProjectAgentRoom]
    func addMember(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        draft: ProjectAgentRoomMemberDraft
    ) async throws -> ProjectAgentRoomMember
    func listMembers(ownerUserID: String, roomID: String) async throws -> [ProjectAgentRoomMember]
    func setDefaultAgent(ownerUserID: String, roomID: String, agentID: String) async throws -> ProjectAgentRoom
    func postMessage(
        ownerUserID: String,
        roomID: String,
        draft: ProjectAgentMessageDraft,
        limits: AgentGroupChatRoutingLimits
    ) async throws -> AgentGroupChatPostResult
    func messageAttachment(
        ownerUserID: String,
        roomID: String,
        messageID: String,
        attachmentID: String
    ) async throws -> ProjectAgentMessageAttachmentPayload?
    func listMessages(
        ownerUserID: String,
        roomID: String,
        afterUnixMs: Int64?,
        limit: Int
    ) async throws -> [ProjectAgentMessage]
    func pageMessages(
        ownerUserID: String,
        roomID: String,
        afterMessageID: String?,
        limit: Int
    ) async throws -> ProjectAgentMessagePage
    func listUnreadMessages(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        limit: Int
    ) async throws -> ProjectAgentUnreadPage
    func markMessagesRead(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        throughMessageID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentReadCursor
    func message(
        ownerUserID: String,
        roomID: String,
        messageID: String
    ) async throws -> ProjectAgentMessage?
    func claimNextDelivery(
        ownerUserID: String,
        agentID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery?
    func delivery(
        ownerUserID: String,
        deliveryID: String
    ) async throws -> ProjectAgentDelivery?
    func completeDelivery(
        ownerUserID: String,
        deliveryID: String,
        responseMessageID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery
    func failDelivery(
        ownerUserID: String,
        deliveryID: String,
        error: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery
}

public enum AgentGroupChatValidation {
    public static func identifier(_ value: String, field: String) throws {
        guard !value.isEmpty,
              value.count <= 512,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              value.rangeOfCharacter(from: .controlCharacters) == nil else {
            throw AgentGroupChatError.invalidField(field)
        }
    }

    public static func identifiers(_ values: [String], field: String, maximumCount: Int) throws {
        guard values.count <= maximumCount, Set(values).count == values.count else {
            throw AgentGroupChatError.invalidField(field)
        }
        for value in values { try identifier(value, field: field) }
    }

    public static func text(_ value: String, field: String, maximumLength: Int) throws {
        guard !value.isEmpty,
              value.count <= maximumLength,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.contains("\0") else {
            throw AgentGroupChatError.invalidField(field)
        }
    }

    public static func optionalText(_ value: String, field: String, maximumLength: Int) throws {
        guard value.count <= maximumLength,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.contains("\0") else {
            throw AgentGroupChatError.invalidField(field)
        }
    }

    public static func timestamps(_ createdAtUnixMs: Int64, _ updatedAtUnixMs: Int64) throws {
        guard createdAtUnixMs >= 0, updatedAtUnixMs >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("timestamps")
        }
    }
}
