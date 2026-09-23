import ChatOSCore
import SwiftUI

@MainActor
enum PetActivityPresentation {
    static func genericMessage(for activity: PetActivity, model: AppModel) -> String {
        switch activity.kind {
        case .waitingForUser: model.localized("打开对应输入表单后即可继续任务。", english: "Open the input form to continue the task.")
        case .working, .reviewing: model.localized("任务仍在执行，可以打开查看完整过程。", english: "The task is still running. Open it to view the full process.")
        case .succeeded: model.localized("任务已经完成，可以打开查看结果。", english: "The task is complete. Open it to view the result.")
        case .cancelled: model.localized("任务已经取消。", english: "The task was cancelled.")
        case .waitingForApproval: model.localized("打开审批详情进行处理。", english: "Open approval details to decide.")
        case .failed, .blocked: model.localized("打开任务详情进行处理。", english: "Open task details to resolve it.")
        }
    }

    static func riskLabel(_ risk: String, model: AppModel) -> String {
        switch risk.lowercased() {
        case "high", "critical": model.localized("高风险", english: "High Risk")
        case "medium": model.localized("中风险", english: "Medium Risk")
        default: model.localized("低风险", english: "Low Risk")
        }
    }

    static func riskColor(_ risk: String) -> Color {
        switch risk.lowercased() {
        case "high", "critical": .red
        case "medium": .orange
        default: .green
        }
    }

    static func messageIcon(for kind: PetActivityKind) -> String {
        switch kind {
        case .waitingForApproval, .waitingForUser: "bell.badge.fill"
        case .failed, .blocked: "exclamationmark.triangle.fill"
        case .succeeded: "checkmark.seal.fill"
        case .reviewing: "eye.fill"
        case .working: "sparkles"
        case .cancelled: "xmark.circle.fill"
        }
    }

    static func messageTint(for kind: PetActivityKind) -> Color {
        switch kind {
        case .waitingForApproval, .waitingForUser: .orange
        case .failed, .blocked: .red
        case .succeeded: .green
        case .reviewing: .purple
        case .working: .accentColor
        case .cancelled: .secondary
        }
    }

    static func displayTitle(for activity: PetActivity, model: AppModel) -> String {
        if let title = displayText(activity.title) {
            return title
        }
        return switch activity.kind {
        case .waitingForApproval: model.localized("有操作等待审批", english: "An Operation Needs Approval")
        case .waitingForUser: model.localized("AI 正在等待你的输入", english: "AI Is Waiting for Your Input")
        case .failed: model.localized("任务执行失败", english: "Task Failed")
        case .blocked: model.localized("任务执行被阻塞", english: "Task Blocked")
        case .succeeded: model.localized("任务已完成", english: "Task Completed")
        case .reviewing: model.localized("AI 正在检查结果", english: "AI Is Reviewing the Result")
        case .working: model.localized("AI 正在处理任务", english: "AI Is Working on the Task")
        case .cancelled: model.localized("任务已取消", english: "Task Cancelled")
        }
    }

    static func displayText(_ value: String?) -> String? {
        guard let value else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty,
              trimmed.unicodeScalars.contains(where: CharacterSet.alphanumerics.contains) else {
            return nil
        }
        return trimmed
    }
}
