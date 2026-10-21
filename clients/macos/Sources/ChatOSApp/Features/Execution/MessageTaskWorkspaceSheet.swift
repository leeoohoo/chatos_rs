import ChatOSCore
import AppKit
import Foundation
import SwiftUI

struct MessageTaskWorkspaceSheet: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: MessageTaskWorkspaceViewModel
    @State private var isFullscreen = false
    @State private var showsGraphOverview = true
    @Environment(\.dismiss) private var dismiss

    init(viewModel: MessageTaskWorkspaceViewModel) {
        _viewModel = StateObject(wrappedValue: viewModel)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if let graph = viewModel.displayGraph, graph.hasOverview {
                graphOverview(graph)
                Divider()
            }
            content
                .workspaceFill()
        }
        .frame(width: workspaceSize.width, height: workspaceSize.height)
        .environment(\.locale, model.interfaceLocale)
        .background(Color(nsColor: .windowBackgroundColor))
        .animation(.easeInOut(duration: 0.2), value: isFullscreen)
        .confirmationDialog(
            model.localized("停止这个任务？", english: "Stop this task?"),
            isPresented: Binding(
                get: { viewModel.cancellationCandidate != nil },
                set: { isPresented in
                    guard !isPresented,
                          viewModel.cancellationCandidate != nil else { return }
                    viewModel.dismissCancellation()
                }
            ),
            titleVisibility: .visible
        ) {
            Button(model.localized("停止任务", english: "Stop Task"), role: .destructive) {
                viewModel.confirmCancellation()
            }
            Button(model.localized("继续执行", english: "Keep Running"), role: .cancel) {
                viewModel.dismissCancellation()
            }
        } message: {
            Text(model.localized(
                "将终止当前执行。浏览器点击、导出或文件操作可能已经发生，系统不会自动重试。",
                english: "This stops the current execution. Browser clicks, exports, or file operations may already have happened and will not be retried automatically."
            ))
        }
        .task { viewModel.load() }
        .onDisappear {
            viewModel.stop()
        }
    }

    private func graphOverview(_ graph: MessageTaskGraphSnapshot) -> some View {
        DisclosureGroup(isExpanded: $showsGraphOverview) {
            VStack(alignment: .leading, spacing: 10) {
                if let goal = graph.goal {
                    overviewField(
                        model.localized("总体目标", english: "Overall Goal"),
                        value: goal
                    )
                }
                if let rationale = graph.decompositionRationale {
                    overviewField(
                        model.localized("模块拆分依据", english: "Decomposition Rationale"),
                        value: rationale
                    )
                }
                if let finalTaskID = graph.finalDeliveryTaskID,
                   let finalTask = graph.nodes.first(where: { $0.id == finalTaskID })?.task {
                    HStack(spacing: 8) {
                        Label(
                            model.localized("最终交付", english: "Final Delivery"),
                            systemImage: "flag.checkered"
                        )
                        .appFont(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                        Text(finalTask.title)
                            .appFont(.caption.weight(.medium))
                    }
                }
            }
            .padding(.top, 10)
        } label: {
            Label(
                model.localized("总体方案", english: "Overall Plan"),
                systemImage: "map.fill"
            )
            .appFont(.subheadline.weight(.semibold))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(AppPalette.ai.opacity(0.045))
    }

    private func overviewField(_ label: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .appFont(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .appFont(.callout)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var header: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text("任务流程图").appFont(.title3.weight(.semibold))
                Text(viewModel.turn.userMessage.text)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            if let graph = viewModel.displayGraph {
                StatusCapsule(title: "任务 \(graph.nodes.count)", color: .secondary)
                StatusCapsule(title: "依赖 \(graph.edges.count)", color: .secondary)
            }
            Picker("图模式", selection: $viewModel.displayMode) {
                Text("精简图").tag(MessageTaskGraphDisplayMode.reduced)
                Text("完整图").tag(MessageTaskGraphDisplayMode.full)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: 150)
            Button("刷新", systemImage: "arrow.clockwise", action: viewModel.refresh)
            if viewModel.hasCancellableTask {
                Button(
                    viewModel.isCancelling ? "正在停止…" : "停止任务",
                    systemImage: "stop.circle"
                ) {
                    viewModel.requestCancellation()
                }
                .disabled(viewModel.isCancelling)
                .tint(.red)
            }
            Button(
                isFullscreen ? "退出全屏" : "全屏",
                systemImage: isFullscreen
                    ? "arrow.down.right.and.arrow.up.left"
                    : "arrow.up.left.and.arrow.down.right"
            ) {
                isFullscreen.toggle()
            }
            .help(isFullscreen ? "恢复默认窗口大小" : "将任务流程图扩展到屏幕可用区域")
            Button("关闭", action: dismiss.callAsFunction)
        }
        .padding(16)
    }

    private var workspaceSize: CGSize {
        let visibleSize = NSScreen.main?.visibleFrame.size ?? CGSize(width: 1440, height: 900)
        if isFullscreen {
            return CGSize(
                width: max(1080, visibleSize.width - 32),
                height: max(680, visibleSize.height - 32)
            )
        }
        return CGSize(
            width: min(1280, max(1080, visibleSize.width - 96)),
            height: min(800, max(720, visibleSize.height - 120))
        )
    }

    @ViewBuilder
    private var content: some View {
        if !viewModel.executionActivity.isEmpty {
            VStack(spacing: 0) {
                TaskExecutionActivityView(viewModel: viewModel)
                    .frame(height: executionActivityHeight)
                Divider()
                graphContent
                    .workspaceFill()
            }
        } else {
            graphContent
        }
    }

    private var executionActivityHeight: CGFloat {
        let visibleRowCount = min(viewModel.executionActivity.count, 3)
        guard visibleRowCount > 0 else { return 88 }

        let headerHeight: CGFloat = 45
        let verticalContentPadding: CGFloat = 28
        let rowHeight: CGFloat = 42
        let rowSpacing = CGFloat(max(0, visibleRowCount - 1)) * 10
        return min(
            220,
            headerHeight + verticalContentPadding + CGFloat(visibleRowCount) * rowHeight + rowSpacing
        )
    }

    @ViewBuilder
    private var graphContent: some View {
        Group {
            if (viewModel.isLoading && viewModel.graph == nil)
                || viewModel.isAwaitingInitialGraph {
                ProgressView("正在同步真实任务图…")
            } else if let error = viewModel.errorMessage, viewModel.graph == nil {
                ContentUnavailableView {
                    Label("任务图加载失败", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(error)
                } actions: {
                    Button("重试", action: viewModel.load)
                }
            } else if let graph = viewModel.displayGraph, !graph.nodes.isEmpty {
                HSplitView {
                    MessageTaskGraphCanvas(
                        graph: graph,
                        selectedTaskID: viewModel.selectedTaskID,
                        onSelect: viewModel.select,
                        onStop: { viewModel.requestCancellation(for: $0) }
                    )
                    .frame(minWidth: 700)
                    MessageTaskInspectorView(viewModel: viewModel)
                }
            } else {
                ContentUnavailableView(
                    "当前消息没有任务图",
                    systemImage: "point.3.connected.trianglepath.dotted",
                    description: Text("AI 尚未通过任务工具为这条消息创建任务节点。")
                )
            }
        }
        .workspaceFill()
    }
}

private extension MessageTaskGraphSnapshot {
    var hasOverview: Bool {
        goal?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
            || decompositionRationale?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
            || finalDeliveryTaskID != nil
    }
}

private struct TaskExecutionActivityView: View {
    @ObservedObject var viewModel: MessageTaskWorkspaceViewModel

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Label("AI 执行过程", systemImage: "sparkles")
                    .appFont(.headline)
                Spacer()
                StatusCapsule(
                    title: "\(viewModel.executionActivity.count) 条记录",
                    color: AppPalette.ai
                )
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 11)
            Divider()

            ScrollView {
                if viewModel.executionActivity.isEmpty {
                    Text("正在等待 AI 返回执行进度…")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(14)
                } else {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(viewModel.executionActivity) { update in
                            HStack(alignment: .top, spacing: 10) {
                                Image(systemName: icon(update.status))
                                    .appFont(.caption.weight(.semibold))
                                    .foregroundStyle(color(update.status))
                                    .frame(width: 18, height: 18)
                                    .background(color(update.status).opacity(0.12), in: Circle())
                                VStack(alignment: .leading, spacing: 3) {
                                    HStack(alignment: .firstTextBaseline) {
                                        Text(update.title).appFont(.callout.weight(.medium))
                                        Spacer(minLength: 12)
                                        if let date = ISO8601DateFormatter().date(from: update.timestamp) {
                                            Text(date, style: .time)
                                                .appFont(.caption2.monospacedDigit())
                                                .foregroundStyle(.tertiary)
                                        }
                                    }
                                    if let detail = update.detail, !detail.isEmpty {
                                        Text(detail)
                                            .appFont(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                            }
                        }
                    }
                    .padding(14)
                }
            }
        }
        .background(AppPalette.canvas)
    }

    private func icon(_ status: String) -> String {
        switch status.lowercased() {
        case "completed": "checkmark"
        case "failed", "cancelled": "exclamationmark"
        default: "arrow.triangle.2.circlepath"
        }
    }

    private func color(_ status: String) -> Color {
        switch status.lowercased() {
        case "completed": .green
        case "failed", "cancelled": .red
        default: AppPalette.ai
        }
    }
}
