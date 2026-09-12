import Foundation

public enum PetActivityRecoveryMapper {
    private static let taskTerminalRetention: TimeInterval = 10 * 60

    public static func applyingAuthoritativeTask(
        _ task: MessageTask,
        to activity: PetActivity,
        now: Date = Date()
    ) -> PetActivity? {
        let status = normalized(task.status?.isEmpty == false ? task.status : task.lastRunStatus)
        let updatedAt = task.updatedAt ?? activity.updatedAt
        let mapping: (kind: PetActivityKind, title: String, detail: String?, expiresAt: Date?)?
        switch status {
        case "running", "processing", "in_progress", "doing", "executing":
            mapping = (
                .working,
                "任务「\(task.title)」正在执行",
                task.resultSummary,
                nil
            )
        case "queued", "queueing":
            mapping = (
                .working,
                "任务「\(task.title)」等待执行",
                nil,
                nil
            )
        case "blocked":
            mapping = terminalMapping(
                .blocked,
                title: "任务「\(task.title)」被阻塞",
                detail: task.resultSummary,
                updatedAt: updatedAt,
                duration: taskTerminalRetention,
                now: now
            )
        case "failed", "error":
            mapping = terminalMapping(
                .failed,
                title: "任务「\(task.title)」执行失败",
                detail: task.resultSummary ?? task.lastRun?.errorMessage,
                updatedAt: updatedAt,
                duration: taskTerminalRetention,
                now: now
            )
        case "completed", "done", "succeeded", "success":
            mapping = terminalMapping(
                .succeeded,
                title: "任务「\(task.title)」已完成",
                detail: task.resultSummary,
                updatedAt: updatedAt,
                duration: taskTerminalRetention,
                now: now
            )
        case "cancelled", "canceled", "stopped":
            mapping = terminalMapping(
                .cancelled,
                title: "任务「\(task.title)」已取消",
                detail: nil,
                updatedAt: updatedAt,
                duration: 5,
                now: now
            )
        default:
            return nil
        }
        guard let mapping else { return nil }
        var reconciled = activity
        reconciled.kind = mapping.kind
        reconciled.title = mapping.title
        reconciled.detail = mapping.detail
        reconciled.updatedAt = updatedAt
        reconciled.expiresAt = mapping.expiresAt
        reconciled.route.runID = task.lastRunID ?? activity.route.runID
        reconciled.route.conversationID = task.sourceSessionID ?? activity.route.conversationID
        reconciled.route.turnID = task.sourceTurnID ?? activity.route.turnID
        return reconciled
    }

    private static func terminalMapping(
        _ kind: PetActivityKind,
        title: String,
        detail: String?,
        updatedAt: Date,
        duration: TimeInterval,
        now: Date
    ) -> (kind: PetActivityKind, title: String, detail: String?, expiresAt: Date?)? {
        let expiresAt = updatedAt.addingTimeInterval(duration)
        guard expiresAt > now else { return nil }
        return (kind, title, detail, expiresAt)
    }

    private static func normalized(_ value: String?) -> String {
        value?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
            .replacingOccurrences(of: "-", with: "_")
            ?? ""
    }
}
