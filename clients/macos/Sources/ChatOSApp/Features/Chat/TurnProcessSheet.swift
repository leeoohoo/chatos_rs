import ChatOSCore
import SwiftUI

struct TurnProcessSheet: View {
    @StateObject private var viewModel: TurnProcessViewModel
    let onClose: () -> Void

    init(viewModel: TurnProcessViewModel, onClose: @escaping () -> Void) {
        _viewModel = StateObject(wrappedValue: viewModel)
        self.onClose = onClose
    }

    var body: some View {
        GeometryReader { viewport in
            workspace
                .frame(width: viewport.size.width, height: viewport.size.height)
                .clipped()
        }
        .onExitCommand(perform: onClose)
        .onAppear(perform: viewModel.load)
        .onDisappear(perform: viewModel.stop)
    }

    private var workspace: some View {
        VStack(spacing: 0) {
            HStack {
                Button("返回聊天", systemImage: "chevron.left", action: onClose)
                Text("任务过程").appFont(.headline)
                Spacer()
                Button("关闭", action: onClose)
            }
            .padding(16)
            Divider()
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
            .workspaceFill()
        }
        .workspaceFill()
        .background(Color(nsColor: .textBackgroundColor))
    }

    private var processTimeline: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
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
