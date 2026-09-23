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
    case requirementSurveyRead = "requirement_survey_read"
    case requirementSurveyWrite = "requirement_survey_write"
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
        self.builtinCapabilities = Self.completingDependencies(in: builtinCapabilities)
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
        guard !builtinCapabilities.contains(.requirementSurveyWrite)
                || builtinCapabilities.contains(.requirementSurveyRead) else {
            throw AgentGroupChatError.invalidField("todoRequirementSurveyReadDependency")
        }
        if !requiresExecution,
           builtinCapabilities.contains(where: {
               $0 != .projectRead && $0 != .requirementSurveyRead
           }) {
            throw AgentGroupChatError.invalidField("todoRequiresExecution")
        }
        try AgentGroupChatValidation.identifier(
            selectionRevision,
            field: "todoCapabilityRevision"
        )
        for plugin in plugins { try plugin.validate() }
    }

    public static func completingDependencies(
        in capabilities: [LocalAgentTodoBuiltinCapability]
    ) -> [LocalAgentTodoBuiltinCapability] {
        var result: [LocalAgentTodoBuiltinCapability] = []
        for capability in capabilities {
            if capability == .requirementSurveyWrite,
               !result.contains(.requirementSurveyRead) {
                result.append(.requirementSurveyRead)
            }
            if !result.contains(capability) { result.append(capability) }
        }
        return result
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

/// A Todo executor can propose durable knowledge without receiving permission to mutate the
/// team's shared assets. The explicit project manager reviews the full replacement Markdown and
/// decides whether to apply it with the normal revision-checked team asset tools.
public struct LocalAgentTeamAssetUpdateSuggestion: Codable, Sendable, Equatable {
    public let category: LocalAgentTeamAssetCategory
    public let title: String
    public let markdown: String
    public let rationale: String

    public init(
        category: LocalAgentTeamAssetCategory,
        title: String,
        markdown: String,
        rationale: String
    ) {
        self.category = category
        self.title = title
        self.markdown = markdown
        self.rationale = rationale
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(
            title,
            field: "teamAssetSuggestionTitle",
            maximumLength: 240
        )
        try AgentGroupChatValidation.text(
            markdown,
            field: "teamAssetSuggestionMarkdown",
            maximumLength: 128_000
        )
        try AgentGroupChatValidation.text(
            rationale,
            field: "teamAssetSuggestionRationale",
            maximumLength: 4_000
        )
    }
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
    public let assetUpdateSuggestions: [LocalAgentTeamAssetUpdateSuggestion]
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
        assetUpdateSuggestions: [LocalAgentTeamAssetUpdateSuggestion] = [],
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
        self.assetUpdateSuggestions = assetUpdateSuggestions
        self.createdAtUnixMs = createdAtUnixMs
    }

    private enum CodingKeys: String, CodingKey {
        case id, ownerUserID, agentID, todoID, sequence, kind, runID, stage, detail
        case assetUpdateSuggestions, createdAtUnixMs
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decode(String.self, forKey: .id)
        ownerUserID = try values.decode(String.self, forKey: .ownerUserID)
        agentID = try values.decode(String.self, forKey: .agentID)
        todoID = try values.decode(String.self, forKey: .todoID)
        sequence = try values.decode(Int64.self, forKey: .sequence)
        kind = try values.decode(LocalAgentTodoProgressKind.self, forKey: .kind)
        runID = try values.decodeIfPresent(String.self, forKey: .runID)
        stage = try values.decode(String.self, forKey: .stage)
        detail = try values.decode(String.self, forKey: .detail)
        assetUpdateSuggestions = try values.decodeIfPresent(
            [LocalAgentTeamAssetUpdateSuggestion].self,
            forKey: .assetUpdateSuggestions
        ) ?? []
        createdAtUnixMs = try values.decode(Int64.self, forKey: .createdAtUnixMs)
    }
}
