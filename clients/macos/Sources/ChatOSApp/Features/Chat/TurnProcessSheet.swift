import ChatOSCore
import SwiftUI

struct TurnProcessSheet: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: TurnProcessViewModel
    @Environment(\.dismiss) private var dismiss

    init(turn: ConversationTurn, service: any TurnProcessServicing) {
        _viewModel = StateObject(
            wrappedValue: TurnProcessViewModel(turn: turn, service: service)
        )
    }

    var body: some View {
        NavigationStack {
            Group {
                if viewModel.isLoading && viewModel.nodes.isEmpty {
                    ProgressView("正在加载任务节点…")
                } else if let errorMessage = viewModel.errorMessage {
                    ContentUnavailableView {
                        Label("任务过程加载失败", systemImage: "exclamationmark.triangle")
                    } description: {
                        Text(errorMessage)
                    } actions: {
                        Button("重试", action: viewModel.load)
                    }
                } else if viewModel.nodes.isEmpty {
                    ContentUnavailableView(
                        "没有可展示的任务节点",
                        systemImage: "point.3.connected.trianglepath.dotted",
                        description: Text("该消息的详细过程接口没有返回任务或工具节点。")
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
        .task { viewModel.load() }
    }

    private var processTimeline: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                header
                ForEach(Array(viewModel.nodes.enumerated()), id: \.element.id) { index, node in
                    ProcessNodeRow(
                        node: node,
                        showsConnector: index < viewModel.nodes.count - 1
                    )
                }
            }
            .padding(24)
        }
        .background(Color(nsColor: .textBackgroundColor))
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(viewModel.turn.userMessage.text)
                .appFont(.headline)
                .lineLimit(3)
            Text("\(viewModel.nodes.count) 个真实过程节点")
                .appFont(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.bottom, 22)
    }
}

private struct ProcessNodeRow: View {
    let node: TurnProcessNode
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
                HStack {
                    Text(node.title).appFont(.subheadline.weight(.semibold))
                    Spacer()
                    if let timestamp = node.timestamp {
                        Text(timestamp, style: .time)
                            .appFont(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                }
                if let detail = node.detail {
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
        switch node.kind {
        case .task: "checklist"
        case .tool: "wrench.and.screwdriver"
        case .reasoning: "brain"
        case .update: "arrow.triangle.2.circlepath"
        }
    }

    private var color: Color {
        switch node.status {
        case .completed: .green
        case .failed, .cancelled: .red
        case .queued: .secondary
        case .streaming: AppPalette.ai
        }
    }
}
