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
    public let defaultPluginIDs: [String]
    public let defaultSkillIDs: [String]

    public init(
        name: String,
        description: String = "",
        rolePrompt: String,
        modelConfigID: String,
        defaultPluginIDs: [String] = [],
        defaultSkillIDs: [String] = []
    ) {
        self.name = name
        self.description = description
        self.rolePrompt = rolePrompt
        self.modelConfigID = modelConfigID
        self.defaultPluginIDs = defaultPluginIDs
        self.defaultSkillIDs = defaultSkillIDs
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(name, field: "name", maximumLength: 120)
        try AgentGroupChatValidation.optionalText(description, field: "description", maximumLength: 2_000)
        try AgentGroupChatValidation.text(rolePrompt, field: "rolePrompt", maximumLength: 32_000)
        try AgentGroupChatValidation.identifier(modelConfigID, field: "modelConfigID")
        try AgentGroupChatValidation.identifiers(defaultPluginIDs, field: "defaultPluginIDs", maximumCount: 100)
        try AgentGroupChatValidation.identifiers(defaultSkillIDs, field: "defaultSkillIDs", maximumCount: 100)
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
    public let pluginIDs: [String]
    public let rationale: String

    public init(
        name: String,
        role: String,
        responsibility: String = "",
        rolePrompt: String,
        modelConfigID: String,
        pluginIDs: [String] = [],
        rationale: String = ""
    ) {
        self.name = name
        self.role = role
        self.responsibility = responsibility
        self.rolePrompt = rolePrompt
        self.modelConfigID = modelConfigID
        self.pluginIDs = pluginIDs
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
        try AgentGroupChatValidation.identifiers(pluginIDs, field: "pluginIDs", maximumCount: 100)
        try AgentGroupChatValidation.optionalText(rationale, field: "rationale", maximumLength: 4_000)
    }

    public var profileDraft: LocalAgentProfileDraft {
        .init(
            name: name,
            description: responsibility,
            rolePrompt: rolePrompt,
            modelConfigID: modelConfigID,
            defaultPluginIDs: pluginIDs
        )
    }

    public var memberDraft: ProjectAgentRoomMemberDraft {
        .init(role: role, responsibility: responsibility, pluginAllowlist: pluginIDs)
    }
}

public struct LocalAgentBuilderModelOption: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let provider: String
    public let modelName: String

    public init(id: String, name: String, provider: String, modelName: String) {
        self.id = id
        self.name = name
        self.provider = provider
        self.modelName = modelName
    }
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
    public let status: ProjectAgentRoomStatus
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    public init(
        id: String,
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft,
        defaultAgentID: String? = nil,
        status: ProjectAgentRoomStatus = .active,
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.draft = draft
        self.defaultAgentID = defaultAgentID
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

public enum ProjectAgentMessageSenderKind: String, Codable, Sendable {
    case human, agent, system
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

    public init(
        senderKind: ProjectAgentMessageSenderKind,
        senderID: String,
        content: String,
        mentionedAgentIDs: [String] = [],
        replyToMessageID: String? = nil,
        sourceRunID: String? = nil,
        causationID: String? = nil,
        rootMessageID: String? = nil,
        hopCount: Int = 0
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
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(senderID, field: "senderID")
        try AgentGroupChatValidation.text(content, field: "content", maximumLength: 64_000)
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

    public init(
        id: String,
        ownerUserID: String,
        roomID: String,
        draft: ProjectAgentMessageDraft,
        rootMessageID: String,
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
        self.createdAtUnixMs = createdAtUnixMs
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
    func createRoom(
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft
    ) async throws -> ProjectAgentRoom
    func activeRoom(ownerUserID: String, projectID: String) async throws -> ProjectAgentRoom?
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
    func listMessages(
        ownerUserID: String,
        roomID: String,
        afterUnixMs: Int64?,
        limit: Int
    ) async throws -> [ProjectAgentMessage]
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
