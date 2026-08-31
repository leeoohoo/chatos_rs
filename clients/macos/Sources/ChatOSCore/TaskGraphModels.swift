import Foundation

public enum TaskNodeStatus: String, Codable, Sendable, CaseIterable {
    case waiting
    case running
    case completed
    case blocked
    case failed
}

public struct TaskGraphNode: Identifiable, Codable, Sendable, Equatable {
    public let id: String
    public var title: String
    public var subtitle: String
    public var status: TaskNodeStatus
    public var progressText: String

    public init(
        id: String,
        title: String,
        subtitle: String,
        status: TaskNodeStatus,
        progressText: String
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.status = status
        self.progressText = progressText
    }
}

public struct TaskGraphEdge: Identifiable, Codable, Sendable, Equatable {
    public let id: String
    public let sourceID: String
    public let targetID: String
    public let blocksTarget: Bool

    public init(id: String, sourceID: String, targetID: String, blocksTarget: Bool = true) {
        self.id = id
        self.sourceID = sourceID
        self.targetID = targetID
        self.blocksTarget = blocksTarget
    }
}

public struct TaskGraph: Sendable, Equatable {
    public var nodes: [TaskGraphNode]
    public var edges: [TaskGraphEdge]

    public init(nodes: [TaskGraphNode], edges: [TaskGraphEdge]) {
        self.nodes = nodes
        self.edges = edges
    }
}
