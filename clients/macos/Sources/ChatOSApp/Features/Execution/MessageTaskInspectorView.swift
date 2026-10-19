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
        if task.normalizedStatus == "blocked" || task.normalizedStatus == "failed" {
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
            allowsTextSelection: true
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
                if let activity = detail.run.activity {
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
                description: Text("该任务节点尚未产生本地执行 Run。")
            )
        } else {
            Button("加载运行详情", systemImage: "arrow.down.circle") {
                viewModel.ensureInspectorSectionLoaded()
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
        if let disposition = task.merging(run: run).resultDisposition {
            return switch disposition {
            case .executionFailed: model.localized("执行失败", english: "Execution Failed")
            case .blocked: model.localized("已阻塞", english: "Blocked")
            case .evidenceIncomplete: model.localized("核验不完整", english: "Verification Incomplete")
            case .acceptanceFailed: model.localized("未通过验收", english: "Acceptance Not Met")
            case .acceptancePassed: model.localized("验收通过", english: "Acceptance Passed")
            }
        }
        if let activity = run.activity {
            return MessageTaskRunActivityPresentation.title(for: activity.kind) {
                model.localized($0, english: $1)
            }
        }
        let taskStatus = task.normalizedStatus
        let terminalTaskStatuses = [
            "completed", "done", "succeeded", "success",
            "blocked", "failed", "error", "cancelled", "canceled", "stopped",
            "reviewing_followup", "waiting_confirmation",
        ]
        let status = terminalTaskStatuses.contains(taskStatus)
            ? taskStatus
            : (run.status?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
                ?? taskStatus)
        return switch status {
        case "running", "processing", "in_progress", "executing": model.localized("执行中", english: "Running")
        case "queued", "queueing": model.localized("排队中", english: "Queued")
        case "blocked": model.localized("已阻塞", english: "Blocked")
        case "reviewing_followup": model.localized("审查追加要求", english: "Reviewing Follow-up")
        case "waiting_confirmation": model.localized("等待确认", english: "Awaiting Confirmation")
        case "failed", "error": model.localized("任务未完成", english: "Task Incomplete")
        case "completed", "done", "succeeded", "success": model.localized("已完成", english: "Completed")
        case "cancelled", "canceled", "stopped": model.localized("已取消", english: "Cancelled")
        default: status.isEmpty ? model.localized("未知", english: "Unknown") : status
        }
    }
}
