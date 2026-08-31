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

    public init(
        id: String,
        taskID: String,
        status: String? = nil,
        modelPhaseStatus: String? = nil,
        startedAt: Date? = nil,
        finishedAt: Date? = nil,
        resultSummary: String? = nil,
        reportContent: String? = nil,
        errorMessage: String? = nil
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
}

public extension MessageTaskGraphServicing {
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
