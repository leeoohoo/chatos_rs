import Foundation

public struct MessageTaskLookup: Codable, Sendable, Equatable {
    public var sessionID: String?
    public var turnID: String?
    public var sourceUserMessageID: String?

    public init(
        sessionID: String? = nil,
        turnID: String? = nil,
        sourceUserMessageID: String? = nil
    ) {
        self.sessionID = sessionID
        self.turnID = turnID
        self.sourceUserMessageID = sourceUserMessageID
    }
}

public struct MessageTask: Identifiable, Sendable, Equatable {
    public let id: String
    public var title: String
    public var description: String?
    public var objective: String?
    public var status: String?
    public var priority: Int?
    public var tags: [String]
    public var defaultModelConfigID: String?
    public var defaultModelConfig: MessageTaskModelConfigSummary?
    public var creatorUserID: String?
    public var creatorUsername: String?
    public var creatorDisplayName: String?
    public var resultSummary: String?
    public var processLog: String?
    public var lastRunID: String?
    public var lastRunStatus: String?
    public var lastRun: MessageTaskLastRunSummary?
    public var parentTaskID: String?
    public var parentTask: MessageTaskReference?
    public var sourceRunID: String?
    public var sourceRun: MessageTaskLastRunSummary?
    public var sourceSessionID: String?
    public var sourceTurnID: String?
    public var sourceUserMessageID: String?
    public var prerequisiteTaskIDs: [String]
    public var prerequisiteTasks: [MessageTaskReference]
    public var projectTaskID: String?
    public var executionClientRef: String?
    public var dependencyContextRefs: [String]
    public var scheduleJSON: String?
    public var taskToolStateJSON: String?
    public var mcpConfigJSON: String?
    public var inputPayloadJSON: String?
    public var createdAt: Date?
    public var updatedAt: Date?

    public init(
        id: String,
        title: String,
        description: String? = nil,
        objective: String? = nil,
        status: String? = nil,
        priority: Int? = nil,
        tags: [String] = [],
        defaultModelConfigID: String? = nil,
        defaultModelConfig: MessageTaskModelConfigSummary? = nil,
        creatorUserID: String? = nil,
        creatorUsername: String? = nil,
        creatorDisplayName: String? = nil,
        resultSummary: String? = nil,
        processLog: String? = nil,
        lastRunID: String? = nil,
        lastRunStatus: String? = nil,
        lastRun: MessageTaskLastRunSummary? = nil,
        parentTaskID: String? = nil,
        parentTask: MessageTaskReference? = nil,
        sourceRunID: String? = nil,
        sourceRun: MessageTaskLastRunSummary? = nil,
        sourceSessionID: String? = nil,
        sourceTurnID: String? = nil,
        sourceUserMessageID: String? = nil,
        prerequisiteTaskIDs: [String] = [],
        prerequisiteTasks: [MessageTaskReference] = [],
        projectTaskID: String? = nil,
        executionClientRef: String? = nil,
        dependencyContextRefs: [String] = [],
        scheduleJSON: String? = nil,
        taskToolStateJSON: String? = nil,
        mcpConfigJSON: String? = nil,
        inputPayloadJSON: String? = nil,
        createdAt: Date? = nil,
        updatedAt: Date? = nil
    ) {
        self.id = id
        self.title = title
        self.description = description
        self.objective = objective
        self.status = status
        self.priority = priority
        self.tags = tags
        self.defaultModelConfigID = defaultModelConfigID
        self.defaultModelConfig = defaultModelConfig
        self.creatorUserID = creatorUserID
        self.creatorUsername = creatorUsername
        self.creatorDisplayName = creatorDisplayName
        self.resultSummary = resultSummary
        self.processLog = processLog
        self.lastRunID = lastRunID
        self.lastRunStatus = lastRunStatus
        self.lastRun = lastRun
        self.parentTaskID = parentTaskID
        self.parentTask = parentTask
        self.sourceRunID = sourceRunID
        self.sourceRun = sourceRun
        self.sourceSessionID = sourceSessionID
        self.sourceTurnID = sourceTurnID
        self.sourceUserMessageID = sourceUserMessageID
        self.prerequisiteTaskIDs = prerequisiteTaskIDs
        self.prerequisiteTasks = prerequisiteTasks
        self.projectTaskID = projectTaskID
        self.executionClientRef = executionClientRef
        self.dependencyContextRefs = dependencyContextRefs
        self.scheduleJSON = scheduleJSON
        self.taskToolStateJSON = taskToolStateJSON
        self.mcpConfigJSON = mcpConfigJSON
        self.inputPayloadJSON = inputPayloadJSON
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct MessageTaskGraphNode: Identifiable, Sendable, Equatable {
    public var id: String { task.id }
    public var task: MessageTask
    public var depth: Int
    public var isRoot: Bool
    public var isCurrentMessage: Bool
    public var groupedTasks: [MessageTask]

    public init(
        task: MessageTask,
        depth: Int,
        isRoot: Bool,
        isCurrentMessage: Bool,
        groupedTasks: [MessageTask] = []
    ) {
        self.task = task
        self.depth = depth
        self.isRoot = isRoot
        self.isCurrentMessage = isCurrentMessage
        self.groupedTasks = groupedTasks
    }
}

public struct MessageTaskGraphEdge: Identifiable, Sendable, Equatable {
    public let id: String
    public var sourceID: String
    public var targetID: String
    public var kind: String

    public init(id: String, sourceID: String, targetID: String, kind: String = "prerequisite") {
        self.id = id
        self.sourceID = sourceID
        self.targetID = targetID
        self.kind = kind
    }
}

public struct MessageTaskGraphSnapshot: Sendable, Equatable {
    public var rootTaskIDs: [String]
    public var nodes: [MessageTaskGraphNode]
    public var edges: [MessageTaskGraphEdge]
    public var sourceSessionID: String?
    public var sourceTurnID: String?
    public var sourceUserMessageID: String?

    public init(
        rootTaskIDs: [String],
        nodes: [MessageTaskGraphNode],
        edges: [MessageTaskGraphEdge],
        sourceSessionID: String? = nil,
        sourceTurnID: String? = nil,
        sourceUserMessageID: String? = nil
    ) {
        self.rootTaskIDs = rootTaskIDs
        self.nodes = nodes
        self.edges = edges
        self.sourceSessionID = sourceSessionID
        self.sourceTurnID = sourceTurnID
        self.sourceUserMessageID = sourceUserMessageID
    }
}
