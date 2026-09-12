import Foundation

public enum PetActivityRecoveryMapper {
    private static let taskTerminalRetention: TimeInterval = 10 * 60

    public static func activities(
        from states: [LocalAgentTaskState],
        now: Date = Date()
    ) -> [PetActivity] {
        states.compactMap { activity(from: $0, now: now) }
    }

    public static func activity(
        from state: LocalAgentTaskState,
        now: Date = Date()
    ) -> PetActivity? {
        let updatedAt = parseDate(state.run.updatedAt) ?? now
        let task = state.task
        let run = state.run
        let detail = run.terminalOutcome?.stringValue(forKey: "text")
            ?? run.terminalOutcome?.stringValue(forKey: "reason")
            ?? state.modelSteps.last?.content.nonEmpty

        let source: PetActivitySource
        let kind: PetActivityKind
        let title: String
        let expiresAt: Date?
        if let prompt = state.userPrompt, prompt.status.isPending {
            source = .askUserPrompt
            kind = .waitingForUser
            title = "任务「\(task.objective)」等待你的输入"
            expiresAt = nil
        } else {
            source = .taskRunner
            switch run.status {
            case .queued, .modelReady, .retryScheduled:
                kind = .working
                title = "任务「\(task.objective)」等待执行"
                expiresAt = nil
            case .modelRunning, .waitingToolResult, .continuationReady:
                kind = .working
                title = "任务「\(task.objective)」正在执行"
                expiresAt = nil
            case .paused, .needsReview:
                kind = .blocked
                title = "任务「\(task.objective)」需要处理"
                expiresAt = nil
            case .succeeded:
                kind = .succeeded
                title = "任务「\(task.objective)」已完成"
                expiresAt = updatedAt.addingTimeInterval(taskTerminalRetention)
            case .failed:
                kind = .failed
                title = "任务「\(task.objective)」执行失败"
                expiresAt = updatedAt.addingTimeInterval(taskTerminalRetention)
            case .cancelled:
                kind = .cancelled
                title = "任务「\(task.objective)」已取消"
                expiresAt = updatedAt.addingTimeInterval(5)
            }
        }
        if let expiresAt, expiresAt <= now { return nil }

        return PetActivity(
            id: source == .askUserPrompt
                ? "ask-user:\(state.userPrompt?.id ?? run.runID)"
                : "task-runner:\(task.taskID)",
            source: source,
            kind: kind,
            title: title,
            detail: detail,
            route: PetActivityRoute(
                projectID: task.projectID,
                conversationID: task.sourceThreadID,
                turnID: task.sourceTurnID,
                promptID: state.userPrompt?.id,
                taskID: task.taskID,
                runID: run.runID
            ),
            activityVersion: "\(run.runID):\(run.version)",
            updatedAt: updatedAt,
            expiresAt: expiresAt
        )
    }

    private static func parseDate(_ value: String) -> Date? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }
}

private extension String {
    var nonEmpty: String? {
        let normalized = trimmingCharacters(in: .whitespacesAndNewlines)
        return normalized.isEmpty ? nil : normalized
    }
}
