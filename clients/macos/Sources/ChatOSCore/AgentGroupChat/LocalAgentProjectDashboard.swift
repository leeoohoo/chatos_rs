import Foundation

public enum LocalAgentProjectHealth: String, Codable, Sendable, CaseIterable {
    case onTrack = "on_track"
    case atRisk = "at_risk"
    case blocked
    case completed
}

public enum LocalAgentProjectMilestoneStatus: String, Codable, Sendable, CaseIterable {
    case pending
    case inProgress = "in_progress"
    case blocked
    case completed
}

public enum LocalAgentProjectIssueSeverity: String, Codable, Sendable, CaseIterable {
    case info, warning, critical
}

public enum LocalAgentProjectIssueOwner: String, Codable, Sendable, CaseIterable {
    case human, agent, external
}

public struct LocalAgentProjectMilestone: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let title: String
    public let detail: String
    public let status: LocalAgentProjectMilestoneStatus
    public let progressPercent: Int
    public let acceptanceCriteria: [String]
    public let linkedTodoIDs: [String]
    public let targetAtUnixMs: Int64?

    public init(
        id: String,
        title: String,
        detail: String = "",
        status: LocalAgentProjectMilestoneStatus,
        progressPercent: Int,
        acceptanceCriteria: [String] = [],
        linkedTodoIDs: [String] = [],
        targetAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.title = title
        self.detail = detail
        self.status = status
        self.progressPercent = progressPercent
        self.acceptanceCriteria = acceptanceCriteria
        self.linkedTodoIDs = linkedTodoIDs
        self.targetAtUnixMs = targetAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "dashboardMilestoneID")
        try AgentGroupChatValidation.text(title, field: "dashboardMilestoneTitle", maximumLength: 240)
        try AgentGroupChatValidation.optionalText(detail, field: "dashboardMilestoneDetail", maximumLength: 8_000)
        guard (0...100).contains(progressPercent), acceptanceCriteria.count <= 32,
              linkedTodoIDs.count <= 128, Set(linkedTodoIDs).count == linkedTodoIDs.count else {
            throw AgentGroupChatError.invalidField("dashboardMilestone")
        }
        for criterion in acceptanceCriteria {
            try AgentGroupChatValidation.text(
                criterion,
                field: "dashboardMilestoneAcceptanceCriterion",
                maximumLength: 2_000
            )
        }
        try AgentGroupChatValidation.identifiers(
            linkedTodoIDs,
            field: "dashboardMilestoneTodoIDs",
            maximumCount: 128
        )
        if let targetAtUnixMs, targetAtUnixMs < 0 {
            throw AgentGroupChatError.invalidField("dashboardMilestoneTargetAt")
        }
    }
}

public struct LocalAgentProjectIssue: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let title: String
    public let detail: String
    public let requestedAction: String
    public let severity: LocalAgentProjectIssueSeverity
    public let owner: LocalAgentProjectIssueOwner
    public let relatedTodoID: String?

    public init(
        id: String,
        title: String,
        detail: String = "",
        requestedAction: String,
        severity: LocalAgentProjectIssueSeverity,
        owner: LocalAgentProjectIssueOwner,
        relatedTodoID: String? = nil
    ) {
        self.id = id
        self.title = title
        self.detail = detail
        self.requestedAction = requestedAction
        self.severity = severity
        self.owner = owner
        self.relatedTodoID = relatedTodoID
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "dashboardIssueID")
        try AgentGroupChatValidation.text(title, field: "dashboardIssueTitle", maximumLength: 240)
        try AgentGroupChatValidation.optionalText(detail, field: "dashboardIssueDetail", maximumLength: 8_000)
        try AgentGroupChatValidation.text(
            requestedAction,
            field: "dashboardIssueRequestedAction",
            maximumLength: 4_000
        )
        if let relatedTodoID {
            try AgentGroupChatValidation.identifier(relatedTodoID, field: "dashboardIssueTodoID")
        }
    }
}

/// Project-manager-maintained semantic layer. Live task, Run, survey and proposal facts remain
/// system-owned and are merged by the UI/tool response instead of being copied into this record.
public struct LocalAgentProjectDashboard: Codable, Sendable, Equatable, Identifiable {
    public var id: String { teamRoomID }
    public let ownerUserID: String
    public let teamRoomID: String
    public let phase: String
    public let health: LocalAgentProjectHealth
    public let summary: String
    public let nextSteps: [String]
    public let milestones: [LocalAgentProjectMilestone]
    public let issues: [LocalAgentProjectIssue]
    public let revision: Int
    public let updatedByAgentID: String
    public let updatedAtUnixMs: Int64

    public init(
        ownerUserID: String,
        teamRoomID: String,
        phase: String,
        health: LocalAgentProjectHealth,
        summary: String,
        nextSteps: [String],
        milestones: [LocalAgentProjectMilestone],
        issues: [LocalAgentProjectIssue],
        revision: Int,
        updatedByAgentID: String,
        updatedAtUnixMs: Int64
    ) {
        self.ownerUserID = ownerUserID
        self.teamRoomID = teamRoomID
        self.phase = phase
        self.health = health
        self.summary = summary
        self.nextSteps = nextSteps
        self.milestones = milestones
        self.issues = issues
        self.revision = revision
        self.updatedByAgentID = updatedByAgentID
        self.updatedAtUnixMs = updatedAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        try AgentGroupChatValidation.identifier(updatedByAgentID, field: "dashboardEditorAgentID")
        try AgentGroupChatValidation.text(phase, field: "dashboardPhase", maximumLength: 240)
        try AgentGroupChatValidation.text(summary, field: "dashboardSummary", maximumLength: 16_000)
        guard revision > 0, updatedAtUnixMs >= 0, nextSteps.count <= 32,
              milestones.count <= 64, issues.count <= 64,
              Set(milestones.map(\.id)).count == milestones.count,
              Set(issues.map(\.id)).count == issues.count else {
            throw AgentGroupChatError.invalidField("projectDashboard")
        }
        for step in nextSteps {
            try AgentGroupChatValidation.text(step, field: "dashboardNextStep", maximumLength: 2_000)
        }
        try milestones.forEach { try $0.validate() }
        try issues.forEach { try $0.validate() }
    }
}

public struct LocalAgentProjectDashboardUpdate: Codable, Sendable, Equatable {
    public let phase: String
    public let health: LocalAgentProjectHealth
    public let summary: String
    public let nextSteps: [String]
    public let milestones: [LocalAgentProjectMilestone]
    public let issues: [LocalAgentProjectIssue]

    public init(
        phase: String,
        health: LocalAgentProjectHealth,
        summary: String,
        nextSteps: [String] = [],
        milestones: [LocalAgentProjectMilestone] = [],
        issues: [LocalAgentProjectIssue] = []
    ) {
        self.phase = phase
        self.health = health
        self.summary = summary
        self.nextSteps = nextSteps
        self.milestones = milestones
        self.issues = issues
    }
}
