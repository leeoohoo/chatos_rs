import ChatOSCore
import SwiftUI

struct TurnProcessSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    let turn: ConversationTurn

    var body: some View {
        NavigationStack {
            Group {
                if turn.processEvents.isEmpty {
                    ContentUnavailableView(
                        "没有可展示的本地过程",
                        systemImage: "point.3.connected.trianglepath.dotted",
                        description: Text("Local Host 尚未为这一轮记录模型、工具或人工交互事件。")
                    )
                } else {
                    processTimeline
                }
            }
            .navigationTitle("任务过程")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("关闭", action: dismiss.callAsFunction)
                }
            }
        }
        .frame(minWidth: 680, minHeight: 560)
        .environment(\.locale, model.interfaceLocale)
    }

    private var processTimeline: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                VStack(alignment: .leading, spacing: 8) {
                    Text(turn.userMessage.text)
                        .appFont(.headline)
                        .lineLimit(3)
                    Text("\(turn.processEvents.count) 个本地过程节点")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(.bottom, 22)

                ForEach(Array(turn.processEvents.enumerated()), id: \.element.id) { index, event in
                    ProcessEventRow(
                        event: event,
                        showsConnector: index < turn.processEvents.count - 1
                    )
                }
            }
            .padding(24)
        }
        .background(Color(nsColor: .textBackgroundColor))
    }
}

private struct ProcessEventRow: View {
    let event: TurnProcessEvent
    let showsConnector: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            VStack(spacing: 0) {
                Image(systemName: symbol)
                    .appFont(.system(size: 13, weight: .semibold))
                    .foregroundStyle(color)
                    .frame(width: 30, height: 30)
                    .background(color.opacity(0.11), in: Circle())
                if showsConnector {
                    Rectangle()
                        .fill(Color.secondary.opacity(0.2))
                        .frame(width: 1, height: 58)
                }
            }

            VStack(alignment: .leading, spacing: 7) {
                Text(event.title).appFont(.subheadline.weight(.semibold))
                if let detail = event.detail, !detail.isEmpty {
                    Text(detail)
                        .appFont(.callout)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .lineLimit(6)
                }
            }
            .padding(.top, 5)
            .padding(.bottom, showsConnector ? 16 : 0)
        }
    }

    private var symbol: String {
        if event.id.hasPrefix("local-agent-tool-") { return "wrench.and.screwdriver" }
        if event.id.hasPrefix("local-agent-reasoning-") { return "brain" }
        if event.id.hasPrefix("local-agent-interaction-") { return "person.crop.circle.badge.questionmark" }
        if event.id.hasPrefix("local-agent-memory-") { return "externaldrive.badge.checkmark" }
        if event.id.hasPrefix("local-agent-run-") { return "sparkles" }
        return "arrow.triangle.2.circlepath"
    }

    private var color: Color {
        switch event.status {
        case .completed: .green
        case .failed, .cancelled: .red
        case .queued: .secondary
        case .streaming: AppPalette.ai
        }
    }
}
