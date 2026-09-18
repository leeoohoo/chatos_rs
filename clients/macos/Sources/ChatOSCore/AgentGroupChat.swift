import Foundation

public enum LocalAgentTodoStatus: String, Codable, Sendable, CaseIterable {
    case pending
    case inProgress = "in_progress"
    case blocked
    case completed
    case cancelled

    public var isTerminal: Bool { self == .completed || self == .cancelled }
}

/// Stable semantic capabilities which a model may request without learning any project, MCP or
/// Plugin identifier. The client resolves these values against the bound team project.
public enum LocalAgentTodoBuiltinCapability: String, Codable, Sendable, CaseIterable, Hashable {
    case projectRead = "project_read"
    case projectWrite = "project_write"
    case terminal
}

/// Program-resolved Plugin selection. `pluginID` is persisted for execution but is never encoded
/// into model-facing Todo responses; the model selects a run-scoped opaque reference instead.
public struct LocalAgentTodoPluginSelection: Codable, Sendable, Equatable {
    public let pluginID: String
    public let displayName: String
    public let reason: String

    public init(pluginID: String, displayName: String, reason: String = "") {
        self.pluginID = pluginID
        self.displayName = displayName
        self.reason = reason
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(pluginID, field: "todoPluginID")
        try AgentGroupChatValidation.text(
            displayName,
            field: "todoPluginDisplayName",
            maximumLength: 240
        )
        try AgentGroupChatValidation.optionalText(
            reason,
            field: "todoPluginReason",
            maximumLength: 1_000
        )
    }
}

/// Trusted execution snapshot created by the client at Todo creation time. The model states the
/// semantic need; the client resolves and validates concrete local Plugins before persisting it.
public struct LocalAgentTodoExecutionPlan: Codable, Sendable, Equatable {
    public let requiresExecution: Bool
    public let builtinCapabilities: [LocalAgentTodoBuiltinCapability]
    public let plugins: [LocalAgentTodoPluginSelection]
    public let selectionRevision: String
    public let selectedAtUnixMs: Int64

    public init(
        requiresExecution: Bool = true,
        builtinCapabilities: [LocalAgentTodoBuiltinCapability] = [.projectRead],
        plugins: [LocalAgentTodoPluginSelection] = [],
        selectionRevision: String = "local-v1",
        selectedAtUnixMs: Int64 = 0
    ) {
        self.requiresExecution = requiresExecution
        self.builtinCapabilities = builtinCapabilities
        self.plugins = plugins
        self.selectionRevision = selectionRevision
        self.selectedAtUnixMs = selectedAtUnixMs
    }

    public func validate() throws {
        guard Set(builtinCapabilities).count == builtinCapabilities.count,
              Set(plugins.map(\.pluginID)).count == plugins.count,
              builtinCapabilities.count <= LocalAgentTodoBuiltinCapability.allCases.count,
              plugins.count <= 32,
              selectedAtUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("todoExecutionPlan")
        }
        if !requiresExecution,
           builtinCapabilities.contains(where: { $0 != .projectRead }) {
            throw AgentGroupChatError.invalidField("todoRequiresExecution")
        }
        try AgentGroupChatValidation.identifier(
            selectionRevision,
            field: "todoCapabilityRevision"
        )
        for plugin in plugins { try plugin.validate() }
    }
}

public enum LocalAgentTodoSourceRelation: String, Codable, Sendable, CaseIterable {
    case created, updated, reprioritized, blockedContext = "blocked_context"
}

/// Internal one-to-many provenance link. Conversation and message identifiers never cross the
/// model boundary; Relay converts them to run-scoped opaque references.
public struct LocalAgentTodoSourceLink: Codable, Sendable, Equatable, Identifiable {
    public var id: String { "\(conversationID):\(messageID):\(relation.rawValue)" }
    public let todoID: String
    public let conversationID: String
    public let messageID: String
    public let relation: LocalAgentTodoSourceRelation
    public let createdAtUnixMs: Int64

    public init(
        todoID: String,
        conversationID: String,
        messageID: String,
        relation: LocalAgentTodoSourceRelation,
        createdAtUnixMs: Int64
    ) {
        self.todoID = todoID
        self.conversationID = conversationID
        self.messageID = messageID
        self.relation = relation
        self.createdAtUnixMs = createdAtUnixMs
    }
}

public struct LocalAgentTodoSourceDraft: Codable, Sendable, Equatable {
    public let roomID: String
    public let messageID: String
    public let relation: LocalAgentTodoSourceRelation

    public init(
        roomID: String,
        messageID: String,
        relation: LocalAgentTodoSourceRelation = .created
    ) {
        self.roomID = roomID
        self.messageID = messageID
        self.relation = relation
    }
}

/// Program-owned edge in the local Todo DAG. The model only sees run-scoped `todo_ref` values;
/// Relay resolves them to these identifiers and the store validates team scope and acyclicity.
public struct LocalAgentTodoDependency: Codable, Sendable, Equatable, Identifiable {
    public var id: String { "\(todoID):\(prerequisiteTodoID)" }
    public let todoID: String
    public let prerequisiteTodoID: String
    public let prerequisiteAgentID: String
    public let createdAtUnixMs: Int64

    public init(
        todoID: String,
        prerequisiteTodoID: String,
        prerequisiteAgentID: String,
        createdAtUnixMs: Int64
    ) {
        self.todoID = todoID
        self.prerequisiteTodoID = prerequisiteTodoID
        self.prerequisiteAgentID = prerequisiteAgentID
        self.createdAtUnixMs = createdAtUnixMs
    }
}

public struct LocalAgentTodoDependencyDraft: Codable, Sendable, Equatable {
    public let prerequisiteTodoID: String
    public let prerequisiteAgentID: String

    public init(prerequisiteTodoID: String, prerequisiteAgentID: String) {
        self.prerequisiteTodoID = prerequisiteTodoID
        self.prerequisiteAgentID = prerequisiteAgentID
    }
}

/// The immutable work contract an executor receives instead of the Agent's manager-chat history.
/// Legacy native callers may omit fields; the Store normalizes title/detail into a minimal contract.
public struct LocalAgentTodoExecutionContract: Codable, Sendable, Equatable {
    public let objective: String
    public let scope: String
    public let expectedOutputs: [String]
    public let acceptanceCriteria: [String]
    public let constraints: [String]

    public init(
        objective: String = "",
        scope: String = "",
        expectedOutputs: [String] = [],
        acceptanceCriteria: [String] = [],
        constraints: [String] = []
    ) {
        self.objective = objective
        self.scope = scope
        self.expectedOutputs = expectedOutputs
        self.acceptanceCriteria = acceptanceCriteria
        self.constraints = constraints
    }

    public func normalized(title: String, detail: String) -> Self {
        .init(
            objective: objective.isEmpty ? title : objective,
            scope: scope.isEmpty ? detail : scope,
            expectedOutputs: expectedOutputs.isEmpty ? [title] : expectedOutputs,
            acceptanceCriteria: acceptanceCriteria.isEmpty
                ? ["完成任务目标并输出可核验的结果总结。"]
                : acceptanceCriteria,
            constraints: constraints
        )
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(
            objective,
            field: "todoObjective",
            maximumLength: 8_000
        )
        try AgentGroupChatValidation.optionalText(scope, field: "todoScope", maximumLength: 16_000)
        try Self.validateList(expectedOutputs, field: "todoExpectedOutputs")
        try Self.validateList(acceptanceCriteria, field: "todoAcceptanceCriteria")
        try Self.validateList(constraints, field: "todoConstraints", allowEmpty: true)
    }

    private static func validateList(
        _ values: [String],
        field: String,
        allowEmpty: Bool = false
    ) throws {
        guard values.count <= 64, allowEmpty || !values.isEmpty else {
            throw AgentGroupChatError.invalidField(field)
        }
        for value in values {
            try AgentGroupChatValidation.text(value, field: field, maximumLength: 4_000)
        }
    }

    private enum CodingKeys: String, CodingKey {
        case objective, scope, expectedOutputs, acceptanceCriteria, constraints
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        objective = try container.decodeIfPresent(String.self, forKey: .objective) ?? ""
        scope = try container.decodeIfPresent(String.self, forKey: .scope) ?? ""
        expectedOutputs = try container.decodeIfPresent([String].self, forKey: .expectedOutputs) ?? []
        acceptanceCriteria = try container.decodeIfPresent(
            [String].self,
            forKey: .acceptanceCriteria
        ) ?? []
        constraints = try container.decodeIfPresent([String].self, forKey: .constraints) ?? []
    }
}

/// A durable item on a project team's shared work board. `teamRoomID` is the ownership boundary;
/// `agentID` is only the current assignee. Source identifiers are host-owned and are replaced with
/// run-scoped references whenever the record is returned to a model.
public struct LocalAgentTodo: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let agentID: String
    public let teamRoomID: String
    public let sourceRoomID: String?
    public let sourceMessageID: String?
    public let title: String
    public let detail: String
    public let priority: Int
    public let sortOrder: Int64
    public let status: LocalAgentTodoStatus
    public let blockedReason: String
    public let result: String
    public let executionContract: LocalAgentTodoExecutionContract
    public let executionPlan: LocalAgentTodoExecutionPlan
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    public init(
        id: String,
        ownerUserID: String,
        agentID: String,
        teamRoomID: String,
        sourceRoomID: String? = nil,
        sourceMessageID: String? = nil,
        title: String,
        detail: String = "",
        priority: Int = 50,
        sortOrder: Int64,
        status: LocalAgentTodoStatus = .pending,
        blockedReason: String = "",
        result: String = "",
        executionContract: LocalAgentTodoExecutionContract = .init(),
        executionPlan: LocalAgentTodoExecutionPlan = .init(),
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.agentID = agentID
        self.teamRoomID = teamRoomID
        self.sourceRoomID = sourceRoomID
        self.sourceMessageID = sourceMessageID
        self.title = title
        self.detail = detail
        self.priority = priority
        self.sortOrder = sortOrder
        self.status = status
        self.blockedReason = blockedReason
        self.result = result
        self.executionContract = executionContract
        self.executionPlan = executionPlan
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "todoID")
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamID")
        if let sourceRoomID {
            try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        }
        if let sourceMessageID {
            try AgentGroupChatValidation.identifier(sourceMessageID, field: "sourceMessageID")
        }
        guard (0...100).contains(priority), sortOrder >= 0 else {
            throw AgentGroupChatError.invalidField("todoPriority")
        }
        try AgentGroupChatValidation.text(title, field: "todoTitle", maximumLength: 500)
        try AgentGroupChatValidation.optionalText(detail, field: "todoDetail", maximumLength: 16_000)
        try AgentGroupChatValidation.optionalText(
            blockedReason,
            field: "todoBlockedReason",
            maximumLength: 8_000
        )
        try AgentGroupChatValidation.optionalText(result, field: "todoResult", maximumLength: 16_000)
        try executionContract.validate()
        try executionPlan.validate()
        try AgentGroupChatValidation.timestamps(createdAtUnixMs, updatedAtUnixMs)
    }
}

/// Program-computed scheduling state for one Agent. `readyTodo` is derived from pending status and
/// completed prerequisites; it is never accepted from a model argument.
public struct LocalAgentTodoScheduleState: Sendable, Equatable {
    public let runningTodo: LocalAgentTodo?
    public let readyTodo: LocalAgentTodo?

    public init(runningTodo: LocalAgentTodo?, readyTodo: LocalAgentTodo?) {
        self.runningTodo = runningTodo
        self.readyTodo = readyTodo
    }
}

public enum LocalAgentTeamAssetCategory: String, Codable, Sendable, CaseIterable {
    case overview
    case currentProgress = "current_progress"
    case techStack = "tech_stack"
    case architecture
    case conventions
    case decision
    case reference
}

public enum LocalAgentTeamAssetStatus: String, Codable, Sendable {
    case active, archived
}

/// Versioned Markdown maintained by a team's explicit project manager. Agent identifiers are kept
/// for audit only and are replaced with run-scoped references at the model boundary.
public struct LocalAgentTeamAsset: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let teamRoomID: String
    public let category: LocalAgentTeamAssetCategory
    public let title: String
    public let markdown: String
    public let revision: Int
    public let status: LocalAgentTeamAssetStatus
    public let createdByAgentID: String?
    public let updatedByAgentID: String?
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    public init(
        id: String,
        ownerUserID: String,
        teamRoomID: String,
        category: LocalAgentTeamAssetCategory,
        title: String,
        markdown: String,
        revision: Int,
        status: LocalAgentTeamAssetStatus = .active,
        createdByAgentID: String? = nil,
        updatedByAgentID: String? = nil,
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.teamRoomID = teamRoomID
        self.category = category
        self.title = title
        self.markdown = markdown
        self.revision = revision
        self.status = status
        self.createdByAgentID = createdByAgentID
        self.updatedByAgentID = updatedByAgentID
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "teamAssetID")
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamID")
        try AgentGroupChatValidation.text(title, field: "teamAssetTitle", maximumLength: 240)
        try AgentGroupChatValidation.optionalText(
            markdown,
            field: "teamAssetMarkdown",
            maximumLength: 128_000
        )
        if let createdByAgentID {
            try AgentGroupChatValidation.identifier(createdByAgentID, field: "createdByAgentID")
        }
        if let updatedByAgentID {
            try AgentGroupChatValidation.identifier(updatedByAgentID, field: "updatedByAgentID")
        }
        guard revision > 0 else { throw AgentGroupChatError.invalidField("teamAssetRevision") }
        try AgentGroupChatValidation.timestamps(createdAtUnixMs, updatedAtUnixMs)
    }
}

public struct LocalAgentTeamAssetRevision: Codable, Sendable, Equatable, Identifiable {
    public var id: String { "\(assetID):\(revision)" }
    public let assetID: String
    public let revision: Int
    public let title: String
    public let markdown: String
    public let editorAgentID: String?
    public let createdAtUnixMs: Int64

    public init(
        assetID: String,
        revision: Int,
        title: String,
        markdown: String,
        editorAgentID: String?,
        createdAtUnixMs: Int64
    ) {
        self.assetID = assetID
        self.revision = revision
        self.title = title
        self.markdown = markdown
        self.editorAgentID = editorAgentID
        self.createdAtUnixMs = createdAtUnixMs
    }
}

public struct LocalAgentTodoTeamAssetSnapshot: Codable, Sendable, Equatable, Identifiable {
    public var id: String { "\(todoID):\(assetID):\(revision)" }
    public let todoID: String
    public let assetID: String
    public let teamRoomID: String
    public let category: LocalAgentTeamAssetCategory
    public let title: String
    public let markdown: String
    public let revision: Int
    public let capturedAtUnixMs: Int64

    public init(
        todoID: String,
        assetID: String,
        teamRoomID: String,
        category: LocalAgentTeamAssetCategory,
        title: String,
        markdown: String,
        revision: Int,
        capturedAtUnixMs: Int64
    ) {
        self.todoID = todoID
        self.assetID = assetID
        self.teamRoomID = teamRoomID
        self.category = category
        self.title = title
        self.markdown = markdown
        self.revision = revision
        self.capturedAtUnixMs = capturedAtUnixMs
    }
}

public struct LocalAgentTodoDraft: Codable, Sendable, Equatable {
    public let title: String
    public let detail: String
    public let priority: Int
    public let teamRoomID: String?
    public let sourceRoomID: String?
    public let sourceMessageID: String?
    public let additionalSources: [LocalAgentTodoSourceDraft]
    public let dependencies: [LocalAgentTodoDependencyDraft]
    public let executionPlan: LocalAgentTodoExecutionPlan
    public let executionContract: LocalAgentTodoExecutionContract
    /// The manager Agent that turned inbox messages into this team Todo. This is used only for
    /// authorization; the assignee remains the `agentID` passed to `createAgentTodo`.
    public let creatorAgentID: String?

    public init(
        title: String,
        detail: String = "",
        priority: Int = 50,
        teamRoomID: String? = nil,
        sourceRoomID: String? = nil,
        sourceMessageID: String? = nil,
        additionalSources: [LocalAgentTodoSourceDraft] = [],
        dependencies: [LocalAgentTodoDependencyDraft] = [],
        executionPlan: LocalAgentTodoExecutionPlan = .init(),
        executionContract: LocalAgentTodoExecutionContract = .init(),
        creatorAgentID: String? = nil
    ) {
        self.title = title
        self.detail = detail
        self.priority = priority
        self.teamRoomID = teamRoomID
        self.sourceRoomID = sourceRoomID
        self.sourceMessageID = sourceMessageID
        self.additionalSources = additionalSources
        self.dependencies = dependencies
        self.executionPlan = executionPlan
        self.executionContract = executionContract
        self.creatorAgentID = creatorAgentID
    }

}

public struct LocalAgentTodoUpdate: Codable, Sendable, Equatable {
    public let title: String?
    public let detail: String?
    public let priority: Int?
    public let status: LocalAgentTodoStatus?
    public let blockedReason: String?
    public let result: String?
    public let executionContract: LocalAgentTodoExecutionContract?

    public init(
        title: String? = nil,
        detail: String? = nil,
        priority: Int? = nil,
        status: LocalAgentTodoStatus? = nil,
        blockedReason: String? = nil,
        result: String? = nil,
        executionContract: LocalAgentTodoExecutionContract? = nil
    ) {
        self.title = title
        self.detail = detail
        self.priority = priority
        self.status = status
        self.blockedReason = blockedReason
        self.result = result
        self.executionContract = executionContract
    }
}

public enum LocalAgentTodoProgressKind: String, Codable, Sendable, CaseIterable {
    case started
    case progress
    case blocked
    case completed
    case cancelled
}

public struct LocalAgentTodoProgress: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let agentID: String
    public let todoID: String
    public let sequence: Int64
    public let kind: LocalAgentTodoProgressKind
    public let runID: String?
    public let stage: String
    public let detail: String
    public let createdAtUnixMs: Int64

    public init(
        id: String,
        ownerUserID: String,
        agentID: String,
        todoID: String,
        sequence: Int64,
        kind: LocalAgentTodoProgressKind,
        runID: String? = nil,
        stage: String = "",
        detail: String,
        createdAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.agentID = agentID
        self.todoID = todoID
        self.sequence = sequence
        self.kind = kind
        self.runID = runID
        self.stage = stage
        self.detail = detail
        self.createdAtUnixMs = createdAtUnixMs
    }
}

public enum ProjectAgentDeliveryTriggerKind: String, Codable, Sendable {
    case mention, defaultAgent = "default_agent", agentMention = "agent_mention", heartbeat, todo
    case todoStatus = "todo_status"
}

public enum LocalAgentRunLane: String, Codable, Sendable {
    case manager, executor
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

    public var lane: LocalAgentRunLane { triggerKind == .todo ? .executor : .manager }

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
    func createMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentMembershipProposalDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentMembershipProposal
    func listMembershipProposals(
        ownerUserID: String,
        sourceRoomID: String,
        status: LocalAgentMembershipProposalStatus?
    ) async throws -> [LocalAgentMembershipProposal]
    func approveMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentMembershipProposalApproval
    func rejectMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentMembershipProposal
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
    func createManagedRoom(
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft,
        projectManagerAgentID: String
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
    func setProjectManager(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) async throws -> ProjectAgentRoom
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
    /// Reads the newest page first and then walks backwards with a stable message cursor.
    /// Returned messages are always chronological inside each page.
    func pageRecentMessages(
        ownerUserID: String,
        roomID: String,
        beforeMessageID: String?,
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
    func readAllUnreadMessagesAndMarkRead(
        ownerUserID: String,
        agentID: String,
        limit: Int,
        nowUnixMs: Int64
    ) async throws -> [LocalAgentUnreadConversation]
    func listAgentTodos(
        ownerUserID: String,
        agentID: String,
        includeTerminal: Bool
    ) async throws -> [LocalAgentTodo]
    func listTeamAssets(
        ownerUserID: String,
        teamRoomID: String,
        includeArchived: Bool
    ) async throws -> [LocalAgentTeamAsset]
    func teamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String
    ) async throws -> LocalAgentTeamAsset?
    func upsertTeamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String?,
        editorAgentID: String?,
        category: LocalAgentTeamAssetCategory,
        title: String,
        markdown: String,
        expectedRevision: Int?,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTeamAsset
    func archiveTeamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String,
        editorAgentID: String?,
        expectedRevision: Int,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTeamAsset
    func listTeamAssetRevisions(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String,
        limit: Int
    ) async throws -> [LocalAgentTeamAssetRevision]
    func listTodoTeamAssetSnapshots(
        ownerUserID: String,
        todoID: String
    ) async throws -> [LocalAgentTodoTeamAssetSnapshot]
    func todoTeamAssetSnapshot(
        ownerUserID: String,
        todoID: String,
        assetID: String,
        revision: Int
    ) async throws -> LocalAgentTodoTeamAssetSnapshot?
    func listTeamTodos(
        ownerUserID: String,
        teamRoomID: String,
        includeTerminal: Bool
    ) async throws -> [LocalAgentTodo]
    func createAgentTodo(
        ownerUserID: String,
        agentID: String,
        requestKey: String,
        draft: LocalAgentTodoDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTodo
    func updateAgentTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        update: LocalAgentTodoUpdate,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTodo
    func reorderAgentTodos(
        ownerUserID: String,
        agentID: String,
        todoIDs: [String],
        nowUnixMs: Int64
    ) async throws -> [LocalAgentTodo]
    func reorderTeamTodos(
        ownerUserID: String,
        teamRoomID: String,
        todoIDs: [String],
        nowUnixMs: Int64
    ) async throws -> [LocalAgentTodo]
    func listAgentTodoProgress(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        limit: Int
    ) async throws -> [LocalAgentTodoProgress]
    func listAgentTodoSources(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) async throws -> [LocalAgentTodoSourceLink]
    func linkAgentTodoSources(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        sources: [LocalAgentTodoSourceDraft],
        nowUnixMs: Int64
    ) async throws -> [LocalAgentTodoSourceLink]
    func listAgentTodoDependencies(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) async throws -> [LocalAgentTodoDependency]
    func setAgentTodoDependencies(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        dependencies: [LocalAgentTodoDependencyDraft],
        nowUnixMs: Int64
    ) async throws -> [LocalAgentTodoDependency]
    func appendAgentTodoProgress(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        kind: LocalAgentTodoProgressKind,
        runID: String?,
        stage: String,
        detail: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTodoProgress
    func agentTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) async throws -> LocalAgentTodo?
    func todoForDelivery(
        ownerUserID: String,
        deliveryID: String
    ) async throws -> LocalAgentTodo?
    func agentTodoScheduleState(
        ownerUserID: String,
        agentID: String
    ) async throws -> LocalAgentTodoScheduleState
    func startNextReadyAgentTodo(
        ownerUserID: String,
        agentID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery?
    func enqueueAgentTodoStatus(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        excludingAgentID: String?,
        nowUnixMs: Int64
    ) async throws -> [ProjectAgentDelivery]
    func enqueueAgentTodoReady(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery?
    func enqueueReadyDependentAgentTodos(
        ownerUserID: String,
        prerequisiteTodoID: String,
        nowUnixMs: Int64
    ) async throws -> [ProjectAgentDelivery]
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
    func completeHeartbeatDelivery(
        ownerUserID: String,
        deliveryID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery
    func failDelivery(
        ownerUserID: String,
        deliveryID: String,
        error: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery
}
