import ChatOSCore
import SwiftUI

struct TaskProcessTimelineView: View {
    let items: [TaskProcessTimelineItem]
    var allowsTextSelection = true

    var body: some View {
        if items.isEmpty {
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
                    StatusCapsule(title: "\(items.count) 个节点", color: .secondary)
                }
                .padding(.bottom, 14)

                ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                    timelineRow(item, isLast: index == items.count - 1)
                }
            }
        }
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
                    if let occurredAt = item.occurredAt {
                        Text(occurredAt)
                            .appFont(.caption2.monospaced())
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
        case "blocked", "failed", "error": "exclamationmark"
        default: "circle.fill"
        }
    }

    private func statusColor(_ status: String) -> Color {
        switch status.lowercased() {
        case "completed", "succeeded", "success", "done": .green
        case "running", "processing", "in_progress", "doing": AppPalette.ai
        case "blocked": .orange
        case "failed", "error", "cancelled", "canceled": .red
        default: .secondary
        }
    }
}
