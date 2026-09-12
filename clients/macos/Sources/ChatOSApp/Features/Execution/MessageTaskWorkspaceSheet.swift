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
        initialTaskID: String? = nil,
        initialRunID: String? = nil
    ) {
        _viewModel = StateObject(
            wrappedValue: MessageTaskWorkspaceViewModel(
                turn: turn,
                graphService: graphService,
                initialTaskID: initialTaskID,
                initialRunID: initialRunID
            )
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
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
        graphContent
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
                ContentUnavailableView(
                    "当前消息没有任务图",
                    systemImage: "point.3.connected.trianglepath.dotted",
                    description: Text("AI 尚未通过 Task Runner MCP 为这条消息创建任务节点。")
                )
            }
        }
        .workspaceFill()
    }
}
