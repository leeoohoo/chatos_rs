import ChatOSCore
import SwiftUI

struct LocalAgentToolApprovalCardView: View {
    @ObservedObject var conversation: ConversationSessionViewModel
    @EnvironmentObject private var model: AppModel
    let approval: LocalAgentToolApprovalRequest

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 9) {
                Image(systemName: "checkmark.shield.fill")
                    .foregroundStyle(.orange)
                Text(model.localized("等待工具授权", english: "Tool Approval Required"))
                    .appFont(.subheadline.weight(.semibold))
                Spacer()
                Text(effectTitle)
                    .appFont(.caption.weight(.medium))
                    .foregroundStyle(.orange)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(.orange.opacity(0.1), in: Capsule())
            }

            Text(approval.toolName)
                .appFont(.body.weight(.medium))
                .textSelection(.enabled)
            Text(approvalExplanation)
                .appFont(.callout)
                .foregroundStyle(.secondary)
            Text(model.localized(
                "请求指纹：\(shortDigest)",
                english: "Request fingerprint: \(shortDigest)"
            ))
            .appFont(.caption.monospaced())
            .foregroundStyle(.tertiary)
            .textSelection(.enabled)

            if let error = conversation.localAgentControlError(for: approval.invocationID) {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .appFont(.caption)
                    .foregroundStyle(.red)
            }

            HStack(spacing: 9) {
                Button(model.localized("拒绝本次执行", english: "Reject This Run")) {
                    conversation.decideLocalAgentToolApproval(approval, decision: .reject)
                }
                .buttonStyle(.bordered)

                Button(model.localized("允许本次执行", english: "Allow This Run")) {
                    conversation.decideLocalAgentToolApproval(approval, decision: .approve)
                }
                .buttonStyle(.borderedProminent)
                .tint(AppPalette.ai)
            }
            .disabled(conversation.isOperatingOnToolApproval(approval.invocationID))

            if conversation.isOperatingOnToolApproval(approval.invocationID) {
                ProgressView(model.localized(
                    "正在提交决定…",
                    english: "Submitting decision…"
                ))
                .controlSize(.small)
            }
        }
        .padding(14)
        .background(.orange.opacity(0.055), in: RoundedRectangle(cornerRadius: 13))
        .overlay {
            RoundedRectangle(cornerRadius: 13)
                .stroke(.orange.opacity(0.25), lineWidth: 1)
        }
        .accessibilityElement(children: .contain)
    }

    private var effectTitle: String {
        switch approval.effect {
        case .read: model.localized("只读", english: "Read only")
        case .idempotentWrite: model.localized("可重试写入", english: "Idempotent write")
        case .write: model.localized("写入", english: "Write")
        case .billable: model.localized("可能计费", english: "Billable")
        case .terminal: model.localized("终止性操作", english: "Terminal")
        }
    }

    private var approvalExplanation: String {
        switch approval.effect {
        case .read:
            model.localized("这个请求只读取数据。", english: "This request only reads data.")
        case .idempotentWrite:
            model.localized(
                "这个请求会修改数据，但使用稳定调用编号避免重复写入。请确认后仅允许本次执行。",
                english: "This request changes data and uses a stable invocation ID to prevent duplicate writes. Approval applies once."
            )
        case .write:
            model.localized(
                "这个请求会修改本地或外部数据。请确认工具名称与当前任务一致。",
                english: "This request changes local or external data. Confirm that the tool matches the current task."
            )
        case .billable:
            model.localized(
                "这个请求可能产生费用。允许后只执行当前这一次调用。",
                english: "This request may incur a charge. Approval applies only to this invocation."
            )
        case .terminal:
            model.localized(
                "这个操作可能发布、提交或完成不可自动撤回的动作。",
                english: "This operation may publish, submit, or finalize an action that cannot be automatically undone."
            )
        }
    }

    private var shortDigest: String {
        let digest = approval.argumentsDigest
        guard digest.count > 22 else { return digest }
        return "\(digest.prefix(15))…\(digest.suffix(6))"
    }
}

struct LocalAgentRunControlCardView: View {
    @ObservedObject var conversation: ConversationSessionViewModel
    @EnvironmentObject private var model: AppModel
    let control: LocalAgentRunControlState

    var body: some View {
        VStack(alignment: .leading, spacing: 11) {
            HStack(spacing: 9) {
                Image(systemName: statusSymbol)
                    .foregroundStyle(statusColor)
                Text(statusTitle)
                    .appFont(.subheadline.weight(.semibold))
                Spacer()
                Text(model.localized(
                    "第 \(control.iteration) 轮",
                    english: "Iteration \(control.iteration)"
                ))
                .appFont(.caption)
                .foregroundStyle(.secondary)
            }

            if let detail = statusDetail {
                Text(detail)
                    .appFont(.callout)
                    .foregroundStyle(.secondary)
            }

            if let error = conversation.localAgentControlError(for: control.runID) {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .appFont(.caption)
                    .foregroundStyle(.red)
            }

            HStack(spacing: 9) {
                if control.canPause {
                    Button(model.localized("暂停 AI", english: "Pause AI")) {
                        conversation.pauseLocalAgentRun(control)
                    }
                    .buttonStyle(.bordered)
                    .help(model.localized(
                        "在当前安全边界停止后续模型与工具步骤",
                        english: "Stop future model and tool steps at the next safe boundary"
                    ))
                }
                if control.canResume {
                    Button(resumeTitle) {
                        conversation.resumeLocalAgentRun(control)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(control.status == .needsReview ? .orange : AppPalette.ai)
                    .help(resumeHelp)
                }
                if control.canCancel {
                    Button(model.localized("取消本次任务", english: "Cancel This Run")) {
                        conversation.cancelLocalAgentRun(control)
                    }
                    .buttonStyle(.bordered)
                    .foregroundStyle(.red)
                    .help(model.localized(
                        "终止这个 Run；已经完成的外部操作不会被自动撤销",
                        english: "End this Run; completed external actions are not automatically undone"
                    ))
                }
            }
            .disabled(conversation.isOperatingOnRun(control.runID))

            if conversation.isOperatingOnRun(control.runID) {
                ProgressView(model.localized(
                    "正在提交操作…",
                    english: "Submitting action…"
                ))
                .controlSize(.small)
            }
        }
        .padding(14)
        .background(statusColor.opacity(0.045), in: RoundedRectangle(cornerRadius: 13))
        .overlay {
            RoundedRectangle(cornerRadius: 13)
                .stroke(statusColor.opacity(0.2), lineWidth: 1)
        }
        .accessibilityElement(children: .contain)
    }

    private var statusTitle: String {
        switch control.status {
        case .queued: model.localized("AI 已排队", english: "AI Queued")
        case .modelReady: model.localized("准备调用模型", english: "Model Ready")
        case .modelRunning: model.localized("AI 正在处理", english: "AI Working")
        case .waitingToolResult: model.localized("正在执行工具", english: "Running Tools")
        case .continuationReady: model.localized("准备继续下一步", english: "Ready to Continue")
        case .retryScheduled: model.localized("等待重试", english: "Retry Scheduled")
        case .paused: control.requiresUserAnswer
            ? model.localized("等待你的回答", english: "Waiting for Your Answer")
            : model.localized("AI 已暂停", english: "AI Paused")
        case .needsReview: model.localized("需要人工复核", english: "Human Review Required")
        case .succeeded: model.localized("已完成", english: "Completed")
        case .failed: model.localized("失败", english: "Failed")
        case .cancelled: model.localized("已取消", english: "Canceled")
        }
    }

    private var statusDetail: String? {
        if let reason = control.reviewReason { return reason }
        if control.requiresUserAnswer {
            return model.localized(
                "请在上方问题卡片中回答。回答会作为新的视觉协作信息写入同一个 Run。",
                english: "Answer in the question card above. Your response becomes new visual collaboration context in the same Run."
            )
        }
        if control.retryCount > 0 {
            return model.localized(
                "已进行 \(control.retryCount) 次有界重试。",
                english: "\(control.retryCount) bounded retries have been attempted."
            )
        }
        return nil
    }

    private var resumeTitle: String {
        control.status == .needsReview
            ? model.localized("已核对，继续", english: "Reviewed, Continue")
            : model.localized("继续 AI", english: "Resume AI")
    }

    private var resumeHelp: String {
        control.status == .needsReview
            ? model.localized(
                "确认外部结果后继续；不要在未核对时继续，以免重复副作用",
                english: "Continue only after checking the external result to avoid duplicate side effects"
            )
            : model.localized("从已持久化的安全点继续", english: "Resume from the persisted safe point")
    }

    private var statusSymbol: String {
        switch control.status {
        case .paused: "pause.circle.fill"
        case .needsReview: "exclamationmark.triangle.fill"
        case .failed: "xmark.octagon.fill"
        case .cancelled: "xmark.circle.fill"
        case .succeeded: "checkmark.circle.fill"
        default: "sparkles"
        }
    }

    private var statusColor: Color {
        switch control.status {
        case .paused: .secondary
        case .needsReview: .orange
        case .failed, .cancelled: .red
        case .succeeded: .green
        default: AppPalette.ai
        }
    }
}
