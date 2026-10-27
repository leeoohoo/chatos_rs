import ChatOSCore
import SwiftUI

struct MessageTaskInspectorView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: MessageTaskWorkspaceViewModel

    var body: some View {
        VStack(spacing: 0) {
            inspectorHeader
            Divider()
            if let task = viewModel.taskDetail ?? viewModel.selectedTask {
                ScrollView {
                    VStack(alignment: .leading, spacing: 18) {
                        switch viewModel.inspectorSection {
                        case .detail:
                            detail(task)
                        case .process:
                            process(task)
                        case .run:
                            run(task)
                        }
                    }
                    .padding(16)
                }
            } else {
                ContentUnavailableView(
                    "选择任务节点",
                    systemImage: "point.3.connected.trianglepath.dotted",
                    description: Text("选择图中的节点后，可查看过程、详情和运行记录。")
                )
            }
        }
        .frame(minWidth: 320, idealWidth: 360, maxWidth: 420)
        .frame(maxHeight: .infinity)
        .background(AppPalette.canvas)
        .conversationFileLinks(conversationID: viewModel.turn.sessionID)
        .onChange(of: viewModel.inspectorSection) {
            viewModel.ensureInspectorSectionLoaded()
        }
    }

    private var inspectorHeader: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("任务检查器").appFont(.headline)
                Spacer()
                if viewModel.isLoadingInspector { ProgressView().controlSize(.small) }
                if let task = viewModel.taskDetail ?? viewModel.selectedTask,
                   task.isCancellableFromInspector {
                    Menu {
                        Button(role: .destructive) {
                            viewModel.requestCancellation(for: task)
                        } label: {
                            Label("停止任务", systemImage: "stop.circle")
                        }
                    } label: {
                        if viewModel.isCancelling {
                            ProgressView().controlSize(.small)
                        } else {
                            Image(systemName: "ellipsis.circle")
                        }
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                    .disabled(viewModel.isCancelling)
                    .help("任务操作")
                }
            }
            Picker("检查内容", selection: $viewModel.inspectorSection) {
                ForEach(MessageTaskWorkspaceViewModel.InspectorSection.allCases, id: \.self) {
                    Text($0.title(language: model.interfaceLanguage)).tag($0)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
        }
        .padding(16)
    }

    @ViewBuilder
    private func detail(_ task: MessageTask) -> some View {
        TaskInspectorTitle(task: task)
        MessageTaskDetailSections(
            task: task,
            isLoadingModelOutput: viewModel.isLoadingModelOutput,
            allowsTextSelection: true
        )
        if (task.normalizedStatus == "blocked" || task.normalizedStatus == "failed"),
           task.lastRunID != nil {
            blockedActions(task)
        }
    }

    @ViewBuilder
    private func process(_ task: MessageTask) -> some View {
        TaskInspectorTitle(task: task)
        TaskProcessTimelineView(
            items: TaskProcessTimelineBuilder.build(
                processLog: task.processLog,
                taskStatus: task.status
            ),
            allowsTextSelection: true,
            liveActivity: task.lastRun?.activity,
            taskStatus: task.status
        )
    }

    @ViewBuilder
    private func run(_ task: MessageTask) -> some View {
        TaskInspectorTitle(task: task)
        if viewModel.isLoadingRun && viewModel.runDetail == nil {
            ProgressView("正在加载运行详情…")
        } else if let detail = viewModel.runDetail {
            if let output = detail.run.reportContent {
                TaskDetailTextCard(
                    title: "模型输出",
                    text: output,
                    allowsTextSelection: true
                )
            }
            if let summary = detail.run.resultSummary,
               summary.trimmingCharacters(in: .whitespacesAndNewlines)
                != detail.run.reportContent?.trimmingCharacters(in: .whitespacesAndNewlines) {
                TaskDetailTextCard(
                    title: "运行结果摘要",
                    text: summary,
                    allowsTextSelection: true
                )
            }
            VStack(alignment: .leading, spacing: 10) {
                Text("运行信息").appFont(.subheadline.weight(.semibold))
                LabeledContent("状态", value: displayedRunStatus(task: task, run: detail.run))
                if let override = MessageTaskRunActivityPresentation.taskStatusTitleOverride(
                    for: task.status,
                    localized: { model.localized($0, english: $1) }
                ) {
                    LabeledContent("当前阶段", value: override)
                    if let overrideDetail = MessageTaskRunActivityPresentation.taskStatusDetailOverride(
                        for: task.status,
                        localized: { model.localized($0, english: $1) }
                    ) {
                        LabeledContent("详情", value: overrideDetail)
                    }
                } else if let activity = detail.run.activity {
                    LabeledContent("当前阶段") {
                        Text(MessageTaskRunActivityPresentation.title(for: activity.kind) {
                            model.localized($0, english: $1)
                        })
                    }
                    if let operation = activity.operation {
                        LabeledContent("当前操作", value: operation)
                    }
                    if let detail = activity.detail {
                        LabeledContent("详情", value: detail)
                    }
                    if let startedAt = activity.startedAt {
                        LabeledContent("已耗时") {
                            Text(startedAt, style: .timer).monospacedDigit()
                        }
                    }
                    if let lastProgressAt = activity.lastProgressAt {
                        LabeledContent("最近进展") {
                            Text(lastProgressAt, style: .relative)
                        }
                    }
                    stalledToolWarning(activity)
                }
                LabeledContent("运行 ID", value: detail.run.id)
                if let startedAt = detail.run.startedAt {
                    LabeledContent("开始时间") { Text(startedAt, style: .date) + Text(" ") + Text(startedAt, style: .time) }
                }
                if let finishedAt = detail.run.finishedAt {
                    LabeledContent("结束时间") { Text(finishedAt, style: .date) + Text(" ") + Text(finishedAt, style: .time) }
                }
                if let error = detail.run.errorMessage { textSection("错误", error) }
            }
            if !detail.events.isEmpty {
                TaskRunEventTimeline(
                    events: detail.events,
                    allowsTextSelection: true
                )
                if detail.eventsHasMore {
                    Button {
                        viewModel.loadMoreRunEvents()
                    } label: {
                        if viewModel.isLoadingMoreRunEvents {
                            ProgressView().controlSize(.small)
                            Text("正在加载…")
                        } else {
                            Text("加载更早的运行事件")
                        }
                    }
                    .disabled(viewModel.isLoadingMoreRunEvents)
                }
            } else {
                ContentUnavailableView(
                    "暂无运行事件",
                    systemImage: "list.bullet.clipboard",
                    description: Text("该次运行尚未写入诊断事件。")
                )
            }
        } else if task.lastRunID == nil {
            ContentUnavailableView(
                "暂无运行记录",
                systemImage: "play.slash",
                description: Text("该任务节点尚未产生运行记录。")
            )
        } else {
            Button("加载运行详情", systemImage: "arrow.down.circle") {
                viewModel.ensureInspectorSectionLoaded()
            }
        }
    }

    @ViewBuilder
    private func stalledToolWarning(_ activity: MessageTaskRunActivity) -> some View {
        if activity.kind == .waitingTool,
           let lastProgressAt = activity.lastProgressAt ?? activity.startedAt {
            TimelineView(.periodic(from: .now, by: 5)) { context in
                if context.date.timeIntervalSince(lastProgressAt) >= 60 {
                    Label(
                        model.localized(
                            "工具长时间没有报告新阶段。租约心跳仍可能继续，但不代表工具有实际进展；你可以停止任务并检查已发生的副作用。",
                            english: "The tool has not reported a new stage for a while. Lease heartbeats may continue without real progress; you can stop the task and inspect possible side effects."
                        ),
                        systemImage: "exclamationmark.triangle.fill"
                    )
                    .appFont(.caption)
                    .foregroundStyle(.orange)
                }
            }
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
            if let error = viewModel.errorMessage {
                Label(error, systemImage: "exclamationmark.circle.fill")
                    .appFont(.caption)
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
            }
            Button {
                viewModel.retrySelectedRun()
            } label: {
                HStack(spacing: 7) {
                    if viewModel.isRetrying {
                        ProgressView().controlSize(.small)
                        Text("正在提交…")
                    } else {
                        Image(systemName: "arrow.clockwise")
                        Text("重新运行")
                    }
                }
            }
            .buttonStyle(.borderedProminent)
            .disabled(task.lastRunID == nil || viewModel.isRetrying)
        }
        .padding(14)
        .background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
    }

    private func textSection(_ title: String, _ text: String) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title).appFont(.subheadline.weight(.semibold))
            Text(text)
                .appFont(.callout)
                .foregroundStyle(.secondary)
                .appTextSelection(true)
        }
    }

    private func displayedRunStatus(task: MessageTask, run: MessageTaskRun) -> String {
        let merged = task.merging(run: run)
        if merged.resultDisposition != nil {
            return MessageTaskStatusPresentation.title(
                for: merged,
                localized: { model.localized($0, english: $1) }
            )
        }
        if let override = MessageTaskRunActivityPresentation.taskStatusTitleOverride(
            for: task.status,
            localized: { model.localized($0, english: $1) }
        ) {
            return override
        }
        if let activity = run.activity {
            return MessageTaskRunActivityPresentation.title(for: activity.kind) {
                model.localized($0, english: $1)
            }
        }
        let taskPresentation = task.presentationStatus
        let terminalTaskPresentations: Set<MessageTaskPresentationStatus> = [
            .completed, .executionFailed, .resultBlocked, .dependencyBlocked, .blocked,
            .evidenceIncomplete, .acceptanceFailed, .acceptancePassed, .cancelled,
            .reviewingFollowup, .waitingConfirmation,
        ]
        let presentation = terminalTaskPresentations.contains(taskPresentation)
            ? taskPresentation
            : MessageTaskPresentationStatus(taskStatus: run.status)
        return MessageTaskStatusPresentation.title(
            for: presentation,
            localized: { model.localized($0, english: $1) }
        )
    }
}

private extension MessageTask {
    var isCancellableFromInspector: Bool {
        isActive
    }
}
