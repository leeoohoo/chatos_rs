import Foundation

public struct ProjectRequirement: Sendable, Equatable, Hashable, Identifiable {
    public var id: String
    public var projectID: String?
    public var parentRequirementID: String?
    public var type: String
    public var title: String
    public var summary: String?
    public var detail: String?
    public var businessValue: String?
    public var acceptanceCriteria: String?
    public var priority: Int
    public var status: String
    public var updatedAt: Date?

    public init(
        id: String,
        projectID: String?,
        parentRequirementID: String?,
        type: String,
        title: String,
        summary: String?,
        detail: String?,
        businessValue: String?,
        acceptanceCriteria: String?,
        priority: Int,
        status: String,
        updatedAt: Date?
    ) {
        self.id = id
        self.projectID = projectID
        self.parentRequirementID = parentRequirementID
        self.type = type
        self.title = title
        self.summary = summary
        self.detail = detail
        self.businessValue = businessValue
        self.acceptanceCriteria = acceptanceCriteria
        self.priority = priority
        self.status = status
        self.updatedAt = updatedAt
    }
}

public struct ProjectWorkItem: Sendable, Equatable, Hashable, Identifiable {
    public var id: String
    public var requirementID: String?
    public var title: String
    public var detail: String?
    public var status: String
    public var priority: Int
    public var tags: [String]
    public var isPlanningTask: Bool
    public var dueAt: Date?

    public init(
        id: String,
        requirementID: String?,
        title: String,
        detail: String?,
        status: String,
        priority: Int,
        tags: [String],
        isPlanningTask: Bool,
        dueAt: Date? = nil
    ) {
        self.id = id
        self.requirementID = requirementID
        self.title = title
        self.detail = detail
        self.status = status
        self.priority = priority
        self.tags = tags
        self.isPlanningTask = isPlanningTask
        self.dueAt = dueAt
    }
}

public struct ProjectPlanEdge: Sendable, Equatable, Hashable, Identifiable {
    public var id: String { "\(sourceID)->\(targetID):\(kind)" }
    public var sourceID: String
    public var targetID: String
    public var kind: String

    public init(sourceID: String, targetID: String, kind: String) {
        self.sourceID = sourceID
        self.targetID = targetID
        self.kind = kind
    }
}

public struct ProjectPlanCounts: Sendable, Equatable {
    public var total: Int
    public var open: Int
    public var done: Int
    public var blocked: Int

    public init(total: Int, open: Int, done: Int, blocked: Int) {
        self.total = total
        self.open = open
        self.done = done
        self.blocked = blocked
    }
}

public struct ProjectPlanSnapshot: Sendable, Equatable {
    public var projectID: String
    public var requirements: [ProjectRequirement]
    public var workItems: [ProjectWorkItem]
    public var edges: [ProjectPlanEdge]
    public var counts: ProjectPlanCounts

    public init(
        projectID: String,
        requirements: [ProjectRequirement],
        workItems: [ProjectWorkItem],
        edges: [ProjectPlanEdge],
        counts: ProjectPlanCounts
    ) {
        self.projectID = projectID
        self.requirements = requirements
        self.workItems = workItems
        self.edges = edges
        self.counts = counts
    }
}

public struct ProjectRequirementDocument: Sendable, Equatable, Identifiable {
    public var id: String
    public var title: String
    public var type: String
    public var format: String
    public var content: String
    public var version: Int
    public var updatedAt: Date?

    public init(
        id: String,
        title: String,
        type: String,
        format: String,
        content: String,
        version: Int,
        updatedAt: Date? = nil
    ) {
        self.id = id
        self.title = title
        self.type = type
        self.format = format
        self.content = content
        self.version = version
        self.updatedAt = updatedAt
    }
}

public struct ProjectRequirementExecutionLaunch: Sendable, Equatable {
    public var projectID: String
    public var requirementID: String
    public var conversationID: String
    public var executionGroupID: String
    public var messageID: String?
    public var confirmationStatus: String
    public var hasStartedRuns: Bool
    public var overallStatus: String?
    public var contactID: String?
    public var taskCount: Int
    public var includePrerequisiteDependents: Bool
    public var failureKind: String?
    public var failureReason: String?
    public var createdAt: Date?

    public init(
        projectID: String,
        requirementID: String,
        conversationID: String,
        executionGroupID: String,
        messageID: String?,
        confirmationStatus: String,
        hasStartedRuns: Bool,
        overallStatus: String? = nil,
        contactID: String? = nil,
        taskCount: Int = 0,
        includePrerequisiteDependents: Bool = false,
        failureKind: String? = nil,
        failureReason: String? = nil,
        createdAt: Date? = nil
    ) {
        self.projectID = projectID
        self.requirementID = requirementID
        self.conversationID = conversationID
        self.executionGroupID = executionGroupID
        self.messageID = messageID
        self.confirmationStatus = confirmationStatus
        self.hasStartedRuns = hasStartedRuns
        self.overallStatus = overallStatus
        self.contactID = contactID
        self.taskCount = taskCount
        self.includePrerequisiteDependents = includePrerequisiteDependents
        self.failureKind = failureKind
        self.failureReason = failureReason
        self.createdAt = createdAt
    }
}

public protocol ProjectPlanServicing: Sendable {
    func fetchPlan(projectID: String) async throws -> ProjectPlanSnapshot
    func fetchWorkItems(projectID: String, requirementID: String) async throws -> ProjectPlanSnapshot
    func fetchDocuments(projectID: String, requirementID: String) async throws -> [ProjectRequirementDocument]
    func fetchExecution(projectID: String, requirementID: String) async throws -> ProjectRequirementExecutionLaunch?
    func createExecution(
        projectID: String,
        requirementID: String,
        includePrerequisiteDependents: Bool,
        planningFeedback: String?
    ) async throws -> ProjectRequirementExecutionLaunch
}
