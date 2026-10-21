import Foundation

public struct MessageTaskRun: Identifiable, Sendable, Equatable {
    public let id: String
    public var taskID: String
    public var status: String?
    public var modelPhaseStatus: String?
    public var startedAt: Date?
    public var finishedAt: Date?
    public var resultSummary: String?
    public var reportContent: String?
    public var errorMessage: String?
    public var acceptanceReport: MessageTaskAcceptanceReport?
    public var activity: MessageTaskRunActivity?

    public init(
        id: String,
        taskID: String,
        status: String? = nil,
        modelPhaseStatus: String? = nil,
        startedAt: Date? = nil,
        finishedAt: Date? = nil,
        resultSummary: String? = nil,
        reportContent: String? = nil,
        errorMessage: String? = nil,
        acceptanceReport: MessageTaskAcceptanceReport? = nil,
        activity: MessageTaskRunActivity? = nil
    ) {
        self.id = id
        self.taskID = taskID
        self.status = status
        self.modelPhaseStatus = modelPhaseStatus
        self.startedAt = startedAt
        self.finishedAt = finishedAt
        self.resultSummary = resultSummary
        self.reportContent = reportContent
        self.errorMessage = errorMessage
        self.acceptanceReport = acceptanceReport
        self.activity = activity
    }
}

public struct MessageTaskRunEvent: Identifiable, Sendable, Equatable {
    public let id: String
    public var eventType: String
    public var message: String?
    public var createdAt: Date?

    public init(id: String, eventType: String, message: String? = nil, createdAt: Date? = nil) {
        self.id = id
        self.eventType = eventType
        self.message = message
        self.createdAt = createdAt
    }
}

public struct MessageTaskRunDetail: Sendable, Equatable {
    public var task: MessageTask
    public var run: MessageTaskRun
    public var events: [MessageTaskRunEvent]
    public var eventsTotal: Int
    public var eventsHasMore: Bool

    public init(
        task: MessageTask,
        run: MessageTaskRun,
        events: [MessageTaskRunEvent],
        eventsTotal: Int = 0,
        eventsHasMore: Bool = false
    ) {
        self.task = task
        self.run = run
        self.events = events
        self.eventsTotal = eventsTotal
        self.eventsHasMore = eventsHasMore
    }
}

public protocol MessageTaskGraphServicing: Sendable {
    func fetchGraph(messageID: String, lookup: MessageTaskLookup?) async throws -> MessageTaskGraphSnapshot
    func fetchTask(messageID: String, taskID: String, lookup: MessageTaskLookup?) async throws -> MessageTask
    func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail
    func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun
    func cancelTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?,
        reason: String?
    ) async throws
    func restartTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?,
        reason: String
    ) async throws -> MessageTaskRun
}

public extension MessageTaskGraphServicing {
    func restartTask(
        messageID _: String,
        taskID _: String,
        lookup _: MessageTaskLookup?,
        reason _: String
    ) async throws -> MessageTaskRun {
        throw MessageTaskRecoveryError.unavailable
    }

    func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTaskRunDetail {
        try await fetchRun(
            messageID: messageID,
            runID: runID,
            lookup: lookup,
            includeEvents: true,
            eventLimit: 40,
            eventOffset: 0
        )
    }
}

public enum MessageTaskRecoveryError: LocalizedError, Sendable {
    case unavailable

    public var errorDescription: String? {
        switch self {
        case .unavailable:
            "当前任务服务不支持从正在执行的节点恢复。"
        }
    }
}
