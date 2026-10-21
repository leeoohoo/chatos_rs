import ChatOSCore
import SwiftUI

struct TaskReplyInlineInspectorView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: TaskReplyInspectorViewModel
    let selection: TaskReplySelection
    let requestedSection: TaskReplyInspectorSection
    @State private var confirmsRestart = false
    @State private var confirmsCancellation = false

    init(
        selection: TaskReplySelection,
        requestedSection: TaskReplyInspectorSection,
        viewModel: TaskReplyInspectorViewModel
    ) {
        self.selection = selection
        self.requestedSection = requestedSection
        _viewModel = StateObject(wrappedValue: viewModel)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 8) {
                Label(viewModel.section.title(language: model.interfaceLanguage), systemImage: sectionIcon)
                    .appFont(.subheadline.weight(.semibold))
                    .foregroundStyle(AppPalette.ai)
                Spacer()
                if viewModel.isLoading || viewModel.isLoadingModelOutput {
                    ProgressView().controlSize(.small)
                }
                if viewModel.canRestartActiveTask || viewModel.canCancelActiveTask {
                    Menu {
                        if viewModel.canRestartActiveTask {
                            Button {
                                confirmsRestart = true
                            } label: {
                                Label("恢复执行", systemImage: "arrow.clockwise.circle")
                            }
                        }
                        if viewModel.canCancelActiveTask {
                            Button(role: .destructive) {
                                confirmsCancellation = true
                            } label: {
                                Label("停止任务", systemImage: "stop.circle")
                            }
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                    .disabled(viewModel.isRestarting || viewModel.isCancelling)
                    .help(model.localized("任务操作", english: "Task Actions"))
                }
            }

            if let error = viewModel.recoveryErrorMessage {
                Label(error, systemImage: "exclamationmark.triangle")
                    .appFont(.caption)
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
            }

            Divider()
            TaskReplyInspectorContent(viewModel: viewModel)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(AppPalette.ai.opacity(0.16), lineWidth: 1)
        }
        .task {
            viewModel.update(selection: selection)
            viewModel.load()
        }
        .onChange(of: selection.refreshIdentity) {
            viewModel.update(selection: selection)
        }
        .onChange(of: requestedSection) {
            viewModel.selectSection(requestedSection)
        }
        .onDisappear {
            viewModel.stop()
        }
        .confirmationDialog(
            "恢复执行这个任务？",
            isPresented: $confirmsRestart,
            titleVisibility: .visible
        ) {
            Button("终止当前运行并重新执行", role: .destructive) {
                viewModel.restartActiveTask()
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("系统会终止当前 Run 并创建新的执行。之前的浏览器点击、导出、授权或文件修改可能已经发生，请确认重复执行是安全的。")
        }
        .confirmationDialog(
            "停止这个任务？",
            isPresented: $confirmsCancellation,
            titleVisibility: .visible
        ) {
            Button("停止任务", role: .destructive) {
                viewModel.cancelActiveTask()
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("任务和正在运行的插件进程会被终止；已经发生的外部操作不会自动撤销。")
        }
    }

    private var sectionIcon: String {
        switch viewModel.section {
        case .process: "waveform.path.ecg"
        case .detail: "doc.text.magnifyingglass"
        }
    }
}

struct TaskReplyInspectorContent: View {
    @ObservedObject var viewModel: TaskReplyInspectorViewModel

    @ViewBuilder
    var body: some View {
        if viewModel.isLoading && viewModel.task == nil {
            ProgressView("正在加载任务…")
                .frame(maxWidth: .infinity, minHeight: 90)
        } else if let error = viewModel.errorMessage, viewModel.task == nil {
            VStack(alignment: .leading, spacing: 10) {
                Label("任务加载失败", systemImage: "exclamationmark.triangle")
                    .appFont(.subheadline.weight(.semibold))
                    .foregroundStyle(.orange)
                Text(error).appFont(.caption).foregroundStyle(.secondary)
                Button("重试", action: viewModel.load)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        } else if let task = viewModel.task {
            VStack(alignment: .leading, spacing: 18) {
                TaskInspectorTitle(task: task)
                switch viewModel.section {
                case .process:
                    TaskProcessTimelineView(
                        items: viewModel.processTimelineItems,
                        allowsTextSelection: true,
                        liveActivity: task.lastRun?.activity
                    )
                case .detail:
                    taskDetail(task)
                }
            }
        }
    }

    @ViewBuilder
    private func taskDetail(_ task: MessageTask) -> some View {
        MessageTaskDetailSections(
            task: task,
            isLoadingModelOutput: viewModel.isLoadingModelOutput,
            allowsTextSelection: true
        )
        if let modelOutputError = viewModel.modelOutputError {
            Label("模型输出读取失败：\(modelOutputError)", systemImage: "exclamationmark.triangle")
                .appFont(.caption)
                .foregroundStyle(.orange)
        }
        if task.normalizedStatus == "blocked" || task.normalizedStatus == "failed" {
            blockedActions(task)
        }
    }

    private func blockedActions(_ task: MessageTask) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("处理阻塞", systemImage: "exclamationmark.triangle")
                .appFont(.subheadline.weight(.semibold))
                .foregroundStyle(.orange)
            Text("补充希望本次重试遵循的说明。留空也可以直接重新处理。")
                .appFont(.caption)
                .foregroundStyle(.secondary)
            ZStack(alignment: .topLeading) {
                if viewModel.retryInstruction.isEmpty {
                    Text("例如：使用当前日期重新查询，并汇总已确认的信息")
                        .appFont(.body)
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 11)
                        .allowsHitTesting(false)
                }
                TextEditor(text: $viewModel.retryInstruction)
                    .scrollContentBackground(.hidden)
                    .appFont(.body)
                    .padding(7)
                    .frame(minHeight: 88)
            }
            .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 9))
            .overlay {
                RoundedRectangle(cornerRadius: 9)
                    .stroke(AppPalette.border, lineWidth: 1)
            }
            if let retryError = viewModel.retryErrorMessage {
                Label(retryError, systemImage: "exclamationmark.circle.fill")
                    .appFont(.caption)
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
            }
            Button(action: viewModel.retry) {
                HStack(spacing: 7) {
                    if viewModel.isRetrying {
                        ProgressView().controlSize(.small)
                        Text("正在提交…")
                    } else {
                        Image(systemName: "arrow.clockwise")
                        Text("重新处理此节点")
                    }
                }
            }
            .buttonStyle(.borderedProminent)
            .disabled(task.lastRunID == nil || viewModel.isRetrying)
        }
        .padding(14)
        .background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
    }

}
