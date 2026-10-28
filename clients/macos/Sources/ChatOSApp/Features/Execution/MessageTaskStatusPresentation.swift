import ChatOSCore
import SwiftUI

enum MessageTaskStatusPresentation {
    typealias Localizer = (_ chinese: String, _ english: String) -> String

    static func title(
        for task: MessageTask,
        localized: Localizer
    ) -> String {
        if task.presentationStatus == .dependencyBlocked,
           task.dependencyBlockingPrerequisites.contains(where: {
               $0.resultStatus?.trimmingCharacters(in: .whitespacesAndNewlines)
                   .lowercased() == "blocked"
           }) {
            return localized(
                "前置任务交付阻塞 · 未启动",
                "Prerequisite Delivery Blocked · Not Started"
            )
        }
        return title(for: task.presentationStatus, localized: localized)
    }

    static func title(
        for status: MessageTaskPresentationStatus,
        localized: Localizer
    ) -> String {
        switch status {
        case .waiting: localized("等待执行", "Waiting")
        case .running: localized("执行中", "Running")
        case .completed: localized("已完成", "Completed")
        case .executionFailed: localized("执行失败", "Execution Failed")
        case .resultBlocked: localized("执行结束 · 交付阻塞", "Execution Ended · Delivery Blocked")
        case .dependencyBlocked: localized("前置任务未满足 · 未启动", "Prerequisite Not Satisfied · Not Started")
        case .blocked: localized("已阻塞", "Blocked")
        case .evidenceIncomplete: localized("部分完成 · 待核验", "Partially Complete · Verification Pending")
        case .acceptanceFailed: localized("未通过验收", "Acceptance Not Met")
        case .acceptancePassed: localized("验收通过", "Acceptance Passed")
        case .cancelled: localized("已取消", "Cancelled")
        case .reviewingFollowup: localized("审查追加要求", "Reviewing Follow-up")
        case .waitingConfirmation: localized("等待确认", "Awaiting Confirmation")
        case .unknown: localized("未知", "Unknown")
        }
    }

    static func title(
        forRawStatus status: String?,
        localized: Localizer
    ) -> String {
        let presentation = MessageTaskPresentationStatus(taskStatus: status)
        guard presentation == .unknown else {
            return title(for: presentation, localized: localized)
        }
        let fallback = status?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return fallback.isEmpty ? title(for: .unknown, localized: localized) : fallback
    }

    static func color(for task: MessageTask) -> Color {
        color(for: task.presentationStatus)
    }

    static func color(for status: MessageTaskPresentationStatus) -> Color {
        switch status {
        case .completed, .acceptancePassed: .green
        case .running: AppPalette.ai
        case .executionFailed, .acceptanceFailed: .red
        case .resultBlocked, .dependencyBlocked, .blocked, .evidenceIncomplete,
             .reviewingFollowup, .waitingConfirmation: .orange
        case .cancelled, .waiting, .unknown: .secondary
        }
    }
}
