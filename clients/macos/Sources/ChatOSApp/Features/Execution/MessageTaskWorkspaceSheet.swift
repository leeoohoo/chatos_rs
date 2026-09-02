import ChatOSCore
import AppKit
import Foundation
import SwiftUI

struct MessageTaskWorkspaceSheet: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: MessageTaskWorkspaceViewModel
    @State private var isFullscreen = false
    @Environment(\.dismiss) private var dismiss

    init(
        turn: ConversationTurn,
        graphService: any MessageTaskGraphServicing,
        projectExecutionService: (any ProjectExecutionServicing)?,
        realtimeService: (any ConversationRealtimeStreaming)? = nil,
        initialTaskID: String? = nil,
        initialRunID: String? = nil
    ) {
        _viewModel = StateObject(
            wrappedValue: MessageTaskWorkspaceViewModel(
                turn: turn,
                graphService: graphService,
                projectExecutionService: projectExecutionService,
                realtimeService: realtimeService,
                initialTaskID: initialTaskID,
                initialRunID: initialRunID
            )
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            ProjectExecutionStatusBanner(viewModel: viewModel)
            content
                .workspaceFill()
        }
        .frame(width: workspaceSize.width, height: workspaceSize.height)
        .environment(\.locale, model.interfaceLocale)
        .background(Color(nsColor: .windowBackgroundColor))
        .animation(.easeInOut(duration: 0.2), value: isFullscreen)
        .task { viewModel.load() }
        .onDisappear {
            viewModel.stopPolling()
            viewModel.stopRealtime()
        }
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
        if viewModel.executionState.isProjectExecution {
            VStack(spacing: 0) {
                ProjectExecutionActivityView(viewModel: viewModel)
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
                        onSelect: viewModel.select
                    )
                    .frame(minWidth: 700)
                    MessageTaskInspectorView(viewModel: viewModel)
                }
            } else {
                if [.failed, .blocked].contains(viewModel.executionState.phase) {
                    ContentUnavailableView(
                        "执行计划生成失败",
                        systemImage: "exclamationmark.triangle",
                        description: Text(
                            viewModel.executionFailureReason
                                ?? "规划 Agent 未能创建任务节点，请检查上方执行过程后重新生成。"
                        )
                    )
                } else if viewModel.executionState.phase == .planning {
                    ContentUnavailableView(
                        "正在等待第一个任务节点",
                        systemImage: "wand.and.stars",
                        description: Text("规划 Agent 创建任务后，流程图会在这里自动更新。")
                    )
                } else if viewModel.executionState.isProjectExecution {
                    ContentUnavailableView(
                        "未找到关联任务图",
                        systemImage: "link.badge.plus",
                        description: Text("消息声明了项目执行计划，但网关未返回关联节点。请刷新；若仍为空，需要检查执行批次关联。")
                    )
                } else {
                    ContentUnavailableView(
                        "当前消息没有任务图",
                        systemImage: "point.3.connected.trianglepath.dotted",
                        description: Text("AI 尚未通过 Task Runner MCP 为这条消息创建任务节点。")
                    )
                }
            }
        }
        .workspaceFill()
    }
}

private struct ProjectExecutionActivityView: View {
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
