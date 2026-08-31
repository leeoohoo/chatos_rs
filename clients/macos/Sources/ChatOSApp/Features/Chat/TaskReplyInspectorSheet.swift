import ChatOSCore
import SwiftUI

struct TaskReplyInlineInspectorView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: TaskReplyInspectorViewModel
    let selection: TaskReplySelection
    let requestedSection: TaskReplyInspectorSection

    init(
        selection: TaskReplySelection,
        requestedSection: TaskReplyInspectorSection,
        service: any MessageTaskGraphServicing
    ) {
        self.selection = selection
        self.requestedSection = requestedSection
        _viewModel = StateObject(
            wrappedValue: TaskReplyInspectorViewModel(selection: selection, service: service)
        )
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
            }

            Divider()
            TaskReplyInspectorContent(viewModel: viewModel)
        }
        .padding(16)
        .frame(maxWidth: 840, alignment: .leading)
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
    }

    private var sectionIcon: String {
        switch viewModel.section {
        case .process: "waveform.path.ecg"
        case .detail: "doc.text.magnifyingglass"
        }
    }
}

private struct TaskReplyInspectorContent: View {
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
                        items: TaskProcessTimelineBuilder.build(
                            processLog: task.processLog,
                            taskStatus: task.status
                        ),
                        allowsTextSelection: false
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
            allowsTextSelection: false
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
        VStack(alignment: .leading, spacing: 10) {
            Label("处理阻塞", systemImage: "exclamationmark.triangle")
                .appFont(.subheadline.weight(.semibold))
                .foregroundStyle(.orange)
            TextEditor(text: $viewModel.retryInstruction)
                .frame(minHeight: 78)
                .overlay { RoundedRectangle(cornerRadius: 7).stroke(.separator) }
            Button("重新处理此节点", systemImage: "arrow.clockwise", action: viewModel.retry)
                .buttonStyle(.borderedProminent)
                .disabled(task.lastRunID == nil || viewModel.isRetrying)
        }
        .padding(12)
        .background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
    }

}
