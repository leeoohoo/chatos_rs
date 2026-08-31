import Foundation

public enum PetActivityRecoveryMapper {
    private static let legacyTaskTerminalRetention: TimeInterval = 10 * 60

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
                duration: legacyTaskTerminalRetention,
                now: now
            )
        case "failed", "error":
            mapping = terminalMapping(
                .failed,
                title: "任务「\(task.title)」执行失败",
                detail: task.resultSummary ?? task.lastRun?.errorMessage,
                updatedAt: updatedAt,
                duration: legacyTaskTerminalRetention,
                now: now
            )
        case "completed", "done", "succeeded", "success":
            mapping = terminalMapping(
                .succeeded,
                title: "任务「\(task.title)」已完成",
                detail: task.resultSummary,
                updatedAt: updatedAt,
                duration: legacyTaskTerminalRetention,
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

    public static func activities(
        conversationID: String,
        projectID: String?,
        turns: [ConversationTurn],
        now: Date = Date()
    ) -> [PetActivity] {
        let orderedTurns = turns.sorted { lhs, rhs in
            if lhs.sequence != rhs.sequence { return lhs.sequence < rhs.sequence }
            return lhs.startedAt < rhs.startedAt
        }
        var activities: [PetActivity] = []
        var latestCallbacks: [String: (ConversationTurn, ConversationAssistantReply, TaskRunnerCallbackReference)] = [:]

        for turn in orderedTurns {
            for reply in turn.assistantReplies {
                guard let callback = reply.taskCallback else { continue }
                if let current = latestCallbacks[callback.taskID],
                   current.1.message.createdAt > reply.message.createdAt {
                    continue
                } else {
                    latestCallbacks[callback.taskID] = (turn, reply, callback)
                }
            }
        }

        var turnsWithSpecificActivity = Set<String>()
        for (_, value) in latestCallbacks {
            let (turn, reply, callback) = value
            guard let mapping = taskMapping(callback, date: reply.message.createdAt, now: now) else {
                continue
            }
            turnsWithSpecificActivity.insert(turn.id)
            activities.append(PetActivity(
                id: "task-runner:\(callback.taskID)",
                source: .taskRunner,
                kind: mapping.kind,
                title: mapping.title,
                route: PetActivityRoute(
                    projectID: projectID ?? turn.projectExecutionContext?.projectID,
                    conversationID: conversationID,
                    turnID: callback.sourceTurnID ?? turn.id,
                    messageID: reply.message.id,
                    taskID: callback.taskID,
                    runID: callback.runID
                ),
                updatedAt: reply.message.createdAt,
                expiresAt: mapping.expiresAt
            ))
        }

        if let executionTurn = orderedTurns.last(where: {
            $0.projectExecutionContext?.isProjectExecution == true
        }), let context = executionTurn.projectExecutionContext,
           let mapping = projectExecutionMapping(
               context,
               date: executionTurn.completedAt ?? executionTurn.startedAt,
               now: now,
               hasSpecificActivity: turnsWithSpecificActivity.contains(executionTurn.id)
           ) {
            activities.append(PetActivity(
                id: "project-execution:\(context.executionGroupID ?? executionTurn.id)",
                source: .projectExecution,
                kind: mapping.kind,
                title: mapping.title,
                detail: mapping.detail,
                route: PetActivityRoute(
                    projectID: context.projectID ?? projectID,
                    conversationID: conversationID,
                    turnID: executionTurn.id,
                    runID: context.executionGroupID
                ),
                updatedAt: executionTurn.completedAt ?? executionTurn.startedAt,
                expiresAt: mapping.expiresAt
            ))
            turnsWithSpecificActivity.insert(executionTurn.id)
        }

        if let latestTurn = orderedTurns.last,
           !turnsWithSpecificActivity.contains(latestTurn.id),
           let mapping = turnMapping(latestTurn, now: now) {
            activities.append(PetActivity(
                id: "chat:\(conversationID):\(latestTurn.id)",
                source: .chat,
                kind: mapping.kind,
                title: mapping.title,
                route: PetActivityRoute(
                    projectID: latestTurn.projectExecutionContext?.projectID ?? projectID,
                    conversationID: conversationID,
                    turnID: latestTurn.id
                ),
                updatedAt: latestTurn.completedAt ?? latestTurn.startedAt,
                expiresAt: mapping.expiresAt
            ))
        }

        return activities
    }

    private static func taskMapping(
        _ callback: TaskRunnerCallbackReference,
        date: Date,
        now: Date
    ) -> (kind: PetActivityKind, title: String, expiresAt: Date?)? {
        let status = normalized(callback.status ?? callback.event)
        if status.contains("blocked") {
            return transient(
                .blocked,
                title: "任务执行被阻塞",
                date: date,
                duration: legacyTaskTerminalRetention,
                now: now
            )
        }
        if status.contains("failed") || status.contains("error") {
            return transient(
                .failed,
                title: "任务执行失败",
                date: date,
                duration: legacyTaskTerminalRetention,
                now: now
            )
        }
        if status.contains("started") || status.contains("running")
            || status.contains("queued") || status.contains("processing")
            || status.contains("in_progress") {
            return (.working, "任务正在执行", nil)
        }
        if status.contains("completed") || status.contains("succeeded")
            || status == "success" || status == "done" {
            return transient(
                .succeeded,
                title: "任务已完成",
                date: date,
                duration: legacyTaskTerminalRetention,
                now: now
            )
        }
        if status.contains("cancel") || status.contains("stopped") {
            return transient(.cancelled, title: "任务已取消", date: date, duration: 5, now: now)
        }
        return nil
    }

    private static func projectExecutionMapping(
        _ context: ProjectExecutionContext,
        date: Date,
        now: Date,
        hasSpecificActivity: Bool
    ) -> (kind: PetActivityKind, title: String, detail: String?, expiresAt: Date?)? {
        let status = normalized(context.overallStatus ?? context.confirmationStatus)
        if ["awaiting_confirmation", "pending_confirmation", "review_required"].contains(status) {
            return (
                .waitingForUser,
                "执行计划等待确认",
                "请检查任务节点和依赖关系",
                nil
            )
        }
        if status == "blocked" {
            guard !hasSpecificActivity,
                  let transient = transient(
                    .blocked,
                    title: "执行计划被阻塞",
                    date: date,
                    duration: 60,
                    now: now
                  ) else { return nil }
            return (transient.kind, transient.title, nil, transient.expiresAt)
        }
        if status == "failed" || status == "error" {
            guard !hasSpecificActivity,
                  let transient = transient(
                    .failed,
                    title: "执行计划失败",
                    date: date,
                    duration: 30,
                    now: now
                  ) else { return nil }
            return (transient.kind, transient.title, nil, transient.expiresAt)
        }
        if ["confirmed", "processing", "running", "executing", "in_progress"].contains(status) {
            // An execution group is only an aggregate container. Its cached
            // status can lag behind the real task graph, so it must never be
            // presented as a user-cancellable task by itself.
            return nil
        }
        if status == "completed" {
            guard let transient = transient(
                .succeeded,
                title: "执行计划已完成",
                date: date,
                duration: 7,
                now: now
            ) else { return nil }
            return (transient.kind, transient.title, nil, transient.expiresAt)
        }
        if ["cancelled", "canceled", "stopped"].contains(status) {
            guard let transient = transient(
                .cancelled,
                title: "执行计划已停止",
                date: date,
                duration: 5,
                now: now
            ) else { return nil }
            return (transient.kind, transient.title, nil, transient.expiresAt)
        }
        return nil
    }

    private static func turnMapping(
        _ turn: ConversationTurn,
        now: Date
    ) -> (kind: PetActivityKind, title: String, expiresAt: Date?)? {
        switch turn.status {
        case .queued, .streaming:
            return (.working, "AI 正在处理任务", nil)
        case .failed:
            return transient(
                .failed,
                title: "AI 执行失败",
                date: turn.completedAt ?? turn.startedAt,
                duration: 30,
                now: now
            )
        case .completed:
            return transient(
                .succeeded,
                title: "AI 已完成本轮任务",
                date: turn.completedAt ?? turn.startedAt,
                duration: 6,
                now: now
            )
        case .cancelled:
            return transient(
                .cancelled,
                title: "AI 执行已取消",
                date: turn.completedAt ?? turn.startedAt,
                duration: 5,
                now: now
            )
        }
    }

    private static func transient(
        _ kind: PetActivityKind,
        title: String,
        date: Date,
        duration: TimeInterval,
        now: Date
    ) -> (kind: PetActivityKind, title: String, expiresAt: Date?)? {
        let expiresAt = date.addingTimeInterval(duration)
        guard expiresAt > now else { return nil }
        return (kind, title, expiresAt)
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
