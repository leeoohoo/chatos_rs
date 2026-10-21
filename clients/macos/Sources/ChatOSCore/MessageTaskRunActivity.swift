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
    public var lastProgressAt: Date?

    public init(
        kind: MessageTaskRunActivityKind,
        operation: String? = nil,
        detail: String? = nil,
        startedAt: Date? = nil,
        lastProgressAt: Date? = nil
    ) {
        self.kind = kind
        self.operation = operation
        self.detail = detail
        self.startedAt = startedAt
        self.lastProgressAt = lastProgressAt
    }
}

/// A force restart cancels the current Run and may repeat external side effects.
/// A quiet tool is only a diagnostic signal: it may still be legitimately busy.
/// Recovery is exposed only after the runtime has recorded an explicit process,
/// lease, or heartbeat interruption.
public enum MessageTaskRecoveryPolicy {
    public static let stalledToolInterval: TimeInterval = 60

    public static func shouldOfferDiagnosis(
        _ task: MessageTask,
        now: Date = Date()
    ) -> Bool {
        guard ["running", "processing", "in_progress", "doing"]
            .contains(task.normalizedStatus),
              let activity = task.lastRun?.activity,
              activity.kind == .waitingTool,
              let lastProgressAt = activity.lastProgressAt ?? activity.startedAt
        else { return false }
        return now.timeIntervalSince(lastProgressAt) >= stalledToolInterval
    }

    public static func canForceRestart(
        _ task: MessageTask,
        now _: Date = Date()
    ) -> Bool {
        guard ["blocked", "failed", "error"].contains(task.normalizedStatus),
              let error = task.lastRun?.errorMessage?
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased(),
              !error.isEmpty else { return false }
        return explicitInterruptionMarkers.contains(where: error.contains)
    }

    private static let explicitInterruptionMarkers = [
        "execution_interrupted",
        "execution interrupted",
        "process unavailable",
        "process exited",
        "plugin process unavailable",
        "plugin process exited",
        "lease expired",
        "heartbeat lost",
        "任务执行意外中断",
        "进程不可用",
        "进程已退出",
        "插件进程不可用",
        "插件进程已退出",
        "租约已过期",
        "心跳丢失",
    ]
}
