import Foundation

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
