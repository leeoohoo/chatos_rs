import ChatOSCore
import SwiftUI

struct TaskProcessTimelineView: View {
    @EnvironmentObject private var model: AppModel
    let items: [TaskProcessTimelineItem]
    var allowsTextSelection = true
    var liveActivity: MessageTaskRunActivity?
    var taskStatus: String?

    var body: some View {
        if displayedItems.isEmpty {
            ContentUnavailableView(
                "暂无执行过程",
                systemImage: "waveform.path.ecg",
                description: Text("任务写入关键执行节点后，会按时间顺序展示在这里。")
            )
        } else {
            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("执行时间线").appFont(.subheadline.weight(.semibold))
                        Text("任务执行期间记录的关键节点")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    StatusCapsule(title: "\(displayedItems.count) 个节点", color: .secondary)
                }
                .padding(.bottom, 14)

                ForEach(Array(displayedItems.enumerated()), id: \.element.id) { index, item in
                    timelineRow(item, isLast: index == displayedItems.count - 1)
                }
            }
        }
    }

    private var displayedItems: [TaskProcessTimelineItem] {
        guard let live = liveActivityItem else { return items }
        return items + [live]
    }

    private var liveActivityItem: TaskProcessTimelineItem? {
        if let stage = MessageTaskRunActivityPresentation.taskStatusTitleOverride(
            for: taskStatus,
            localized: { model.localized($0, english: $1) }
        ) {
            let detail = MessageTaskRunActivityPresentation.taskStatusDetailOverride(
                for: taskStatus,
                localized: { model.localized($0, english: $1) }
            ) ?? stage
            return .init(
                id: "task-live-gate-\(taskStatus ?? "paused")",
                title: model.localized("实时 · \(stage)", english: "Live · \(stage)"),
                detail: detail,
                occurredAt: liveActivity?.startedAt.map { ISO8601DateFormatter().string(from: $0) },
                status: "paused"
            )
        }
        guard let activity = liveActivity else { return nil }
        let stage = MessageTaskRunActivityPresentation.title(for: activity.kind) {
            model.localized($0, english: $1)
        }
        let operation = MessageTaskRunActivityPresentation.operationLabel(activity.operation)
        var details: [String] = []
        if let operation {
            details.append(model.localized("当前操作：", english: "Current operation: ") + operation)
        } else if activity.kind == .runningModel {
            details.append(model.localized(
                "AI 正在分析上下文并决定下一步。",
                english: "AI is analyzing context and deciding the next step."
            ))
        }
        if let detail = activity.detail?.trimmingCharacters(in: .whitespacesAndNewlines),
           !detail.isEmpty {
            details.append(model.localized("用途或进展：", english: "Purpose or progress: ") + detail)
        }
        if let lastProgressAt = activity.lastProgressAt ?? activity.startedAt {
            details.append(model.localized("最近真实进展：", english: "Last real progress: ")
                + lastProgressAt.formatted(date: .omitted, time: .standard))
        }
        return .init(
            id: "task-live-activity-\(activity.kind.rawValue)-\(activity.operation ?? "stage")",
            title: model.localized("实时 · \(stage)", english: "Live · \(stage)"),
            detail: details.isEmpty
                ? model.localized("任务仍在执行，正在等待下一次状态更新。", english: "The task is still running and waiting for the next status update.")
                : details.joined(separator: "\n"),
            occurredAt: activity.startedAt.map { ISO8601DateFormatter().string(from: $0) },
            status: "running"
        )
    }

    private func timelineRow(_ item: TaskProcessTimelineItem, isLast: Bool) -> some View {
        HStack(alignment: .top, spacing: 12) {
            VStack(spacing: 0) {
                Image(systemName: statusIcon(item.status))
                    .appFont(.caption2.weight(.semibold))
                    .foregroundStyle(statusColor(item.status))
                    .frame(width: 18, height: 18)
                    .background(statusColor(item.status).opacity(0.12), in: Circle())
                if !isLast {
                    Rectangle()
                        .fill(Color(nsColor: .separatorColor))
                        .frame(width: 1, height: 54)
                }
            }
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline) {
                    Text(item.title).appFont(.callout.weight(.semibold))
                    Spacer()
                    if let occurredAt = item.occurredAtDate {
                        if item.id.hasPrefix("task-live-activity-") {
                            Text(occurredAt, style: .timer)
                                .appFont(.caption2.monospacedDigit())
                                .foregroundStyle(.secondary)
                        } else {
                            Text(occurredAt.formatted(date: .omitted, time: .standard))
                                .appFont(.caption2.monospacedDigit())
                                .foregroundStyle(.secondary)
                        }
                    } else if let occurredAt = item.occurredAt {
                        Text(occurredAt)
                            .appFont(.caption2.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                }
                Text(item.detail)
                    .appFont(.callout)
                    .foregroundStyle(.primary.opacity(0.78))
                    .appTextSelection(allowsTextSelection)
                Text(LocalizedStringKey(statusTitle(item.status)))
                    .appFont(.caption2.weight(.medium))
                    .foregroundStyle(statusColor(item.status))
            }
            .padding(.bottom, isLast ? 0 : 14)
        }
    }

    private func statusTitle(_ status: String) -> String {
        switch status.lowercased() {
        case "completed", "succeeded", "success", "done": "已完成"
        case "running", "processing", "in_progress", "doing": "进行中"
        case "paused": "已暂停"
        case "blocked": "阻塞"
        case "failed", "error": "失败"
        case "cancelled", "canceled": "已取消"
        default: "已记录"
        }
    }

    private func statusIcon(_ status: String) -> String {
        switch status.lowercased() {
        case "completed", "succeeded", "success", "done": "checkmark"
        case "running", "processing", "in_progress", "doing": "arrow.triangle.2.circlepath"
        case "paused": "pause.fill"
        case "blocked", "failed", "error": "exclamationmark"
        default: "circle.fill"
        }
    }

    private func statusColor(_ status: String) -> Color {
        switch status.lowercased() {
        case "completed", "succeeded", "success", "done": .green
        case "running", "processing", "in_progress", "doing": AppPalette.ai
        case "paused": .orange
        case "blocked": .orange
        case "failed", "error", "cancelled", "canceled": .red
        default: .secondary
        }
    }
}
