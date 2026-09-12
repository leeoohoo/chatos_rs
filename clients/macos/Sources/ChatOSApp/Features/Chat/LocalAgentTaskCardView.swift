import ChatOSCore
import SwiftUI

struct LocalAgentTaskCardView: View {
    @EnvironmentObject private var model: AppModel
    let state: LocalAgentTaskState
    @State private var showsDetails = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 9) {
                Image(systemName: statusSymbol)
                    .foregroundStyle(statusColor)
                Text(model.localized("本地 AI 任务", english: "Local AI Task"))
                    .appFont(.subheadline.weight(.semibold))
                Spacer()
                Text(statusTitle)
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(statusColor)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(statusColor.opacity(0.1), in: Capsule())
            }

            Text(state.task.objective)
                .appFont(.body.weight(.medium))
                .textSelection(.enabled)

            HStack(spacing: 12) {
                Label(
                    model.localized(
                        "第 \(state.run.stepSeq) 步",
                        english: "Step \(state.run.stepSeq)"
                    ),
                    systemImage: "point.topleft.down.to.point.bottomright.curvepath"
                )
                Label(
                    model.localized(
                        "\(state.tools.count) 个工具调用",
                        english: "\(state.tools.count) tool calls"
                    ),
                    systemImage: "wrench.and.screwdriver"
                )
                if state.run.retryCount > 0 {
                    Label(
                        model.localized(
                            "重试 \(state.run.retryCount) 次",
                            english: "\(state.run.retryCount) retries"
                        ),
                        systemImage: "arrow.clockwise"
                    )
                }
            }
            .appFont(.caption)
            .foregroundStyle(.secondary)

            if let question = state.pendingInteraction?.prompt,
               !question.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Label(question, systemImage: "questionmark.bubble.fill")
                    .appFont(.callout)
                    .foregroundStyle(.orange)
            }

            if let memorySync = LocalAgentTaskMemorySyncPresentation(status: state.memorySync) {
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: memorySync.systemImage)
                        .foregroundStyle(memorySyncColor(memorySync.kind))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(memorySyncTitle(memorySync))
                            .appFont(.caption.weight(.semibold))
                        if let errorCode = memorySync.errorCode {
                            Text(errorCode)
                                .appFont(.caption.monospaced())
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                        }
                    }
                }
                .accessibilityElement(children: .combine)
            }

            if let outcomeSummary {
                VStack(alignment: .leading, spacing: 5) {
                    Text(model.localized("完成结果", english: "Outcome"))
                        .appFont(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Text(outcomeSummary)
                        .appFont(.callout)
                        .textSelection(.enabled)
                }
            }

            DisclosureGroup(isExpanded: $showsDetails) {
                VStack(alignment: .leading, spacing: 12) {
                    detailSection(
                        model.localized("验收标准", english: "Acceptance Criteria"),
                        values: state.task.acceptanceCriteria
                    )
                    if !state.modelSteps.isEmpty {
                        detailSection(
                            model.localized("模型步骤", english: "Model Steps"),
                            values: state.modelSteps.map(modelStepSummary)
                        )
                    }
                    if !state.tools.isEmpty {
                        detailSection(
                            model.localized("工具执行", english: "Tool Execution"),
                            values: state.tools.map(toolSummary)
                        )
                    }
                    HStack(spacing: 6) {
                        Text("project_id")
                            .appFont(.caption.monospaced().weight(.semibold))
                        Text(state.task.projectID)
                            .appFont(.caption.monospaced())
                            .textSelection(.enabled)
                    }
                    .foregroundStyle(.tertiary)
                }
                .padding(.top, 10)
            } label: {
                Text(model.localized("查看任务过程", english: "Show Task Details"))
                    .appFont(.callout.weight(.medium))
            }
        }
        .padding(14)
        .background(statusColor.opacity(0.04), in: RoundedRectangle(cornerRadius: 13))
        .overlay {
            RoundedRectangle(cornerRadius: 13)
                .stroke(statusColor.opacity(0.2), lineWidth: 1)
        }
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder
    private func detailSection(_ title: String, values: [String]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .appFont(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ForEach(Array(values.enumerated()), id: \.offset) { index, value in
                HStack(alignment: .top, spacing: 7) {
                    Text("\(index + 1).")
                        .foregroundStyle(.tertiary)
                    Text(value)
                        .textSelection(.enabled)
                }
                .appFont(.caption)
            }
        }
    }

    private var statusTitle: String {
        switch state.run.status {
        case .queued: model.localized("等待开始", english: "Queued")
        case .modelReady: model.localized("准备模型", english: "Model Ready")
        case .modelRunning: model.localized("设计中", english: "Designing")
        case .waitingToolResult: model.localized("执行工具", english: "Running Tools")
        case .continuationReady: model.localized("准备下一步", english: "Continuing")
        case .retryScheduled: model.localized("等待重试", english: "Retrying")
        case .paused: model.localized("已暂停", english: "Paused")
        case .needsReview: model.localized("需要复核", english: "Needs Review")
        case .succeeded: model.localized("已完成", english: "Completed")
        case .failed: model.localized("失败", english: "Failed")
        case .cancelled: model.localized("已取消", english: "Canceled")
        }
    }

    private var statusSymbol: String {
        switch state.run.status {
        case .queued, .modelReady, .continuationReady: "clock.fill"
        case .modelRunning, .waitingToolResult: "sparkles"
        case .retryScheduled: "arrow.clockwise.circle.fill"
        case .paused: "pause.circle.fill"
        case .needsReview: "exclamationmark.shield.fill"
        case .succeeded: "checkmark.circle.fill"
        case .failed: "xmark.octagon.fill"
        case .cancelled: "slash.circle.fill"
        }
    }

    private var statusColor: Color {
        switch state.run.status {
        case .succeeded: .green
        case .failed, .cancelled: .red
        case .paused, .needsReview, .retryScheduled: .orange
        default: AppPalette.ai
        }
    }

    private var outcomeSummary: String? {
        guard case let .object(values) = state.run.terminalOutcome else { return nil }
        for key in ["text", "summary", "reason", "message"] {
            if case let .string(value) = values[key],
               !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                return value
            }
        }
        return nil
    }

    private func memorySyncTitle(_ presentation: LocalAgentTaskMemorySyncPresentation) -> String {
        switch presentation.kind {
        case .pending:
            model.localized(
                "\(presentation.count) 条任务记忆等待同步",
                english: "\(presentation.count) task memories waiting to sync"
            )
        case .failed:
            model.localized(
                "\(presentation.count) 条任务记忆同步失败",
                english: "\(presentation.count) task memories failed to sync"
            )
        }
    }

    private func memorySyncColor(_ kind: LocalAgentTaskMemorySyncPresentation.Kind) -> Color {
        switch kind {
        case .pending: .orange
        case .failed: .red
        }
    }

    private func modelStepSummary(_ step: LocalAgentTaskModelStepState) -> String {
        let detail = [step.status, step.content, step.reasoning]
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty }
        return model.localized(
            "第 \(step.stepSequence) 步",
            english: "Step \(step.stepSequence)"
        ) + detail.map { " · \($0)" }.orEmpty
    }

    private func toolSummary(_ tool: LocalAgentToolSnapshot) -> String {
        "\(tool.toolName) · \(tool.status.rawValue)"
    }
}

struct LocalAgentTaskMemorySyncPresentation: Equatable {
    enum Kind: Equatable {
        case pending
        case failed
    }

    let kind: Kind
    let count: UInt64
    let errorCode: String?

    var systemImage: String {
        switch kind {
        case .pending: "arrow.triangle.2.circlepath"
        case .failed: "exclamationmark.arrow.triangle.2.circlepath"
        }
    }

    init?(status: LocalAgentMemorySyncStatus?) {
        guard let status else { return nil }
        if status.failedCount > 0 {
            kind = .failed
            count = status.failedCount
            errorCode = status.lastErrorCode
        } else if status.pendingCount > 0 {
            kind = .pending
            count = status.pendingCount
            errorCode = nil
        } else {
            return nil
        }
    }
}

private extension Optional where Wrapped == String {
    var orEmpty: String { self ?? "" }
}
