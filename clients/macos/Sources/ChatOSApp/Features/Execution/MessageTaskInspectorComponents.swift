import ChatOSCore
import SwiftUI

struct TaskInspectorTitle: View {
    @EnvironmentObject private var model: AppModel
    let task: MessageTask

    var body: some View {
        HStack(alignment: .top, spacing: 11) {
            Image(systemName: "checklist")
                .appFont(.system(size: 14, weight: .semibold))
                .foregroundStyle(statusColor)
                .frame(width: 34, height: 34)
                .background(statusColor.opacity(0.11), in: RoundedRectangle(cornerRadius: 9))
            VStack(alignment: .leading, spacing: 5) {
                Text(task.title).appFont(.title3.weight(.semibold))
                if let summary = task.objective ?? task.description {
                    Text(summary)
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            Spacer(minLength: 8)
            StatusCapsule(title: statusTitle, color: statusColor)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var statusTitle: String {
        MessageTaskStatusPresentation.title(
            for: task,
            localized: { model.localized($0, english: $1) }
        )
    }

    private var statusColor: Color {
        MessageTaskStatusPresentation.color(for: task)
    }
}

struct TaskRunEventTimeline: View {
    let events: [MessageTaskRunEvent]
    var allowsTextSelection = true

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("运行事件 · \(events.count)").appFont(.subheadline.weight(.semibold))
            ForEach(events) { event in
                HStack(alignment: .top, spacing: 10) {
                    Circle()
                        .fill(AppPalette.ai.opacity(0.75))
                        .frame(width: 7, height: 7)
                        .padding(.top, 6)
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(event.eventType).appFont(.caption.weight(.semibold))
                            Spacer()
                            if let createdAt = event.createdAt {
                                Text(createdAt, style: .time)
                                    .appFont(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                        }
                        if let message = event.message {
                            Text(message)
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                                .appTextSelection(allowsTextSelection)
                        }
                    }
                }
            }
        }
    }
}
