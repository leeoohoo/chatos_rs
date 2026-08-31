import Foundation

public struct ProjectExecutionContext: Codable, Sendable, Equatable {
    public var projectID: String?
    public var requirementID: String?
    public var executionGroupID: String?
    public var replacedExecutionGroupID: String?
    public var contactID: String?
    public var mode: String?
    public var executionKind: String?
    public var confirmationStatus: String?
    public var overallStatus: String?

    public init(
        projectID: String? = nil,
        requirementID: String? = nil,
        executionGroupID: String? = nil,
        replacedExecutionGroupID: String? = nil,
        contactID: String? = nil,
        mode: String? = nil,
        executionKind: String? = nil,
        confirmationStatus: String? = nil,
        overallStatus: String? = nil
    ) {
        self.projectID = projectID
        self.requirementID = requirementID
        self.executionGroupID = executionGroupID
        self.replacedExecutionGroupID = replacedExecutionGroupID
        self.contactID = contactID
        self.mode = mode
        self.executionKind = executionKind
        self.confirmationStatus = confirmationStatus
        self.overallStatus = overallStatus
    }

    public var isProjectExecution: Bool {
        projectID != nil
            || normalized(mode) == "project_requirement_execution"
            || normalized(executionKind) == "project_requirement_execution"
    }

    private func normalized(_ value: String?) -> String {
        value?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() ?? ""
    }
}

public struct ProjectExecutionIdentity: Sendable, Equatable {
    public var projectID: String
    public var requirementID: String
    public var executionGroupID: String
    public var conversationID: String
    public var contactID: String?

    public init(
        projectID: String,
        requirementID: String,
        executionGroupID: String,
        conversationID: String,
        contactID: String? = nil
    ) {
        self.projectID = projectID
        self.requirementID = requirementID
        self.executionGroupID = executionGroupID
        self.conversationID = conversationID
        self.contactID = contactID
    }
}

public struct ProjectExecutionActionResult: Sendable, Equatable {
    public var success: Bool
    public var status: String?
    public var executionGroupID: String?
    public var taskIDs: [String]
    public var rootTaskIDs: [String]
    public var discardedTasks: Bool?

    public init(
        success: Bool,
        status: String? = nil,
        executionGroupID: String? = nil,
        taskIDs: [String] = [],
        rootTaskIDs: [String] = [],
        discardedTasks: Bool? = nil
    ) {
        self.success = success
        self.status = status
        self.executionGroupID = executionGroupID
        self.taskIDs = taskIDs
        self.rootTaskIDs = rootTaskIDs
        self.discardedTasks = discardedTasks
    }
}

public protocol ProjectExecutionServicing: Sendable {
    func fetchExecution(
        _ identity: ProjectExecutionIdentity
    ) async throws -> ProjectRequirementExecutionLaunch?
    func confirmExecution(_ identity: ProjectExecutionIdentity) async throws -> ProjectExecutionActionResult
    func abandonPlan(_ identity: ProjectExecutionIdentity) async throws -> ProjectExecutionActionResult
}
