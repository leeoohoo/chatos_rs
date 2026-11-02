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
            HStack(spacing: 12) {
                Button(action: onClose) {
                    Image(systemName: "chevron.left")
                        .frame(width: 30, height: 30)
                        .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 8))
                }
                .buttonStyle(.plain)
                Image(systemName: "waveform.path.ecg")
                    .appFont(.system(size: 14, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 34, height: 34)
                    .background(AppPalette.ai, in: RoundedRectangle(cornerRadius: 10))
                VStack(alignment: .leading, spacing: 2) {
                    Text("任务过程").appFont(.headline)
                    Text("按发生顺序查看任务、推理与工具节点")
                        .appFont(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if !viewModel.nodes.isEmpty {
                    StatusCapsule(title: "\(viewModel.nodes.count) 个节点", color: AppPalette.ai)
                }
                Button("关闭", systemImage: "xmark", action: onClose)
                    .labelStyle(.iconOnly)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 14)
            .background(AppPalette.surface)
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
        .background(AppPalette.canvas)
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
            .frame(maxWidth: 920)
            .frame(maxWidth: .infinity)
        }
        .background(AppPalette.canvas)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(viewModel.turn.userMessage.text)
                .appFont(.title3.weight(.semibold))
                .lineLimit(3)
            HStack(spacing: 10) {
                processMetric("全部", value: viewModel.nodes.count, icon: "list.bullet")
                processMetric("工具", value: viewModel.nodes.filter { $0.kind == .tool }.count, icon: "wrench.and.screwdriver")
                processMetric("任务", value: viewModel.nodes.filter { $0.kind == .task }.count, icon: "checklist")
                processMetric("推理", value: viewModel.nodes.filter { $0.kind == .reasoning }.count, icon: "brain")
            }
        }
        .padding(18)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 14))
        .overlay { RoundedRectangle(cornerRadius: 14).stroke(AppPalette.border.opacity(0.6)) }
        .padding(.bottom, 22)
    }

    private func processMetric(_ title: String, value: Int, icon: String) -> some View {
        HStack(spacing: 6) {
            Image(systemName: icon).foregroundStyle(AppPalette.ai)
            Text("\(value)").appFont(.caption.weight(.semibold))
            Text(title).appFont(.caption).foregroundStyle(.secondary)
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 6)
        .background(AppPalette.surfaceSubtle, in: Capsule())
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
                    VStack(alignment: .leading, spacing: 3) {
                        Text(node.title).appFont(.subheadline.weight(.semibold))
                        Text(kindTitle)
                            .appFont(.caption2.weight(.medium))
                            .foregroundStyle(color)
                    }
                    Spacer()
                    if let timestamp = node.timestamp {
                        Text(timestamp, style: .time)
                            .appFont(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                }
                if let detail = node.detail {
                    TurnProcessExpandableDetail(text: detail)
                }
            }
            .padding(12)
            .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 11))
            .overlay { RoundedRectangle(cornerRadius: 11).stroke(AppPalette.border.opacity(0.55)) }
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

    private var kindTitle: String {
        switch node.kind {
        case .task: "任务节点"
        case .tool: "工具调用"
        case .reasoning: "分析与决策"
        case .update: "进度更新"
        }
    }
}

private struct TurnProcessExpandableDetail: View {
    let text: String
    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(text)
                .appFont(.callout)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
                .lineLimit(expanded ? nil : 5)
            if text.count > 260 || text.split(separator: "\n", omittingEmptySubsequences: false).count > 5 {
                Button(expanded ? "收起" : "展开详情") {
                    withAnimation(.easeInOut(duration: 0.16)) { expanded.toggle() }
                }
                .buttonStyle(.plain)
                .appFont(.caption.weight(.medium))
                .foregroundStyle(AppPalette.ai)
            }
        }
    }
}
