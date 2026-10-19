import Foundation

public enum MessageTaskRunActivityKind: String, Sendable, Equatable {
    case waitingDependency
    case waitingModel
    case runningModel
    case waitingTool
    case waitingApproval
    case waitingUser
    case startingProcess
    case readingLargeResult
    case restoringCapability
    case retryScheduled
    case paused
}

public struct MessageTaskRunActivity: Sendable, Equatable {
    public var kind: MessageTaskRunActivityKind
    public var operation: String?
    public var detail: String?
    public var startedAt: Date?

    public init(
        kind: MessageTaskRunActivityKind,
        operation: String? = nil,
        detail: String? = nil,
        startedAt: Date? = nil
    ) {
        self.kind = kind
        self.operation = operation
        self.detail = detail
        self.startedAt = startedAt
    }
}
