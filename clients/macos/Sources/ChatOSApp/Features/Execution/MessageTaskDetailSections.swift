import ChatOSCore
import SwiftUI

struct MessageTaskDetailSections: View {
    @EnvironmentObject private var model: AppModel
    let task: MessageTask
    var isLoadingModelOutput = false
    var allowsTextSelection = true

    var body: some View {
        if let conversationID = task.sourceSessionID {
            sections.conversationFileLinks(conversationID: conversationID)
        } else {
            sections
        }
    }

    @ViewBuilder
    private var sections: some View {
        if let report = task.lastRun?.acceptanceReport {
            TaskDetailDisclosureSection(title: "验收结果", defaultExpanded: true) {
                VStack(alignment: .leading, spacing: 10) {
                    TaskDetailMarkdownText(text: report.reason, allowsTextSelection: allowsTextSelection)
                    ForEach(report.checks, id: \.criterionIndex) { check in
                        TaskDetailNamedText(label: "\(check.criterionIndex + 1). \(check.status == "passed" ? "已通过" : check.status == "failed" ? "未通过" : "未核验")",
                            value: check.evidence, allowsTextSelection: allowsTextSelection)
                    }
                    Text("验收依据来自执行者报告；不等同于浏览器或工具运行状态。")
                        .foregroundStyle(.secondary)
                }
            }
        }
        if let error = task.lastRun?.errorMessage, !error.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            TaskDetailDisclosureSection(title: "阻塞或失败原因", defaultExpanded: true) {
                TaskDetailMarkdownText(text: error, allowsTextSelection: allowsTextSelection)
            }
        }
        modelOutputSection

        if let summary = distinctResultSummary {
            TaskDetailDisclosureSection(title: "执行结果摘要") {
                TaskDetailMarkdownText(
                    text: summary,
                    allowsTextSelection: allowsTextSelection
                )
            }
            .id("result-summary-\(task.id)")
        }

        TaskDetailDisclosureSection(title: "任务内容", defaultExpanded: true) {
            VStack(alignment: .leading, spacing: 14) {
                TaskDetailNamedText(
                    label: "目标",
                    value: task.objective ?? "-",
                    allowsTextSelection: allowsTextSelection
                )
                TaskDetailNamedText(
                    label: "描述",
                    value: task.description ?? "-",
                    allowsTextSelection: allowsTextSelection
                )
            }
        }

        if let plan = MessageTaskRequirementPlan.decode(inputPayloadJSON: task.inputPayloadJSON) {
            TaskDetailDisclosureSection(title: "引用的需求方案") {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text(plan.snapshot.title)
                        Spacer()
                        Text("方案版本")
                        Text(String(plan.snapshot.revision)).monospacedDigit()
                    }
                    Text("这是创建任务时固定的已确认方案，重试仍使用此版本。")
                        .foregroundStyle(.secondary)
                    TaskDetailMarkdownText(text: plan.snapshot.markdown, allowsTextSelection: allowsTextSelection)
                }
            }
        }

        TaskDetailDisclosureSection(title: "更多任务信息") {
            VStack(alignment: .leading, spacing: 20) {
                TaskDetailSubsection(title: "基本信息") {
                    basicInformation
                }

                TaskDetailSubsection(title: "前置任务（\(prerequisiteItems.count)）") {
                    prerequisites
                }

                if let mcpConfig = task.mcpConfigJSON {
                    TaskDetailSubsection(title: "MCP / 工作区 / 服务器") {
                        TaskDetailCodeBlock(
                            text: mcpConfig,
                            allowsTextSelection: allowsTextSelection
                        )
                    }
                }

                if let toolState = task.taskToolStateJSON {
                    TaskDetailSubsection(title: "过程产物") {
                        TaskDetailCodeBlock(
                            text: toolState,
                            allowsTextSelection: allowsTextSelection
                        )
                    }
                }

                TaskDetailSubsection(title: "来源信息") {
                    sourceInformation
                }

                if let schedule = task.scheduleJSON {
                    TaskDetailSubsection(title: "调度配置") {
                        TaskDetailCodeBlock(
                            text: schedule,
                            allowsTextSelection: allowsTextSelection
                        )
                    }
                }

                if let input = task.inputPayloadJSON {
                    TaskDetailSubsection(title: "原始输入") {
                        TaskDetailCodeBlock(
                            text: input,
                            allowsTextSelection: allowsTextSelection
                        )
                    }
                }
            }
        }
    }

    private var modelOutputSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("模型输出", systemImage: "sparkles")
                .appFont(.headline)
                .foregroundStyle(AppPalette.ai)

            if let output = modelOutput {
                TaskDetailMarkdownText(
                    text: output,
                    allowsTextSelection: allowsTextSelection
                )
            } else if isActiveTask {
                HStack(spacing: 8) {
                    if ["reviewing_followup", "waiting_confirmation"].contains(task.normalizedStatus) {
                        Image(systemName: "pause.circle")
                    } else {
                        ProgressView().controlSize(.small)
                    }
                    Text(executionPendingMessage)
                        .appFont(.callout)
                        .foregroundStyle(.secondary)
                }
            } else if isLoadingModelOutput {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("正在读取本次运行的模型输出…")
                        .appFont(.callout)
                        .foregroundStyle(.secondary)
                }
            } else {
                Text(task.lastRunID == nil ? "该任务尚未产生运行记录。" : "本次运行没有返回模型输出。")
                    .appFont(.callout)
                    .foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(AppPalette.ai.opacity(0.07), in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(AppPalette.ai.opacity(0.18), lineWidth: 1)
        }
    }

    private var basicInformation: some View {
        VStack(alignment: .leading, spacing: 8) {
            LabeledContent("任务 ID", value: task.id)
            LabeledContent("状态", value: taskStatusTitle(task.status))
            LabeledContent("创建人", value: creatorDisplayName)
            LabeledContent("模型", value: modelDisplayName)
            LabeledContent("思考等级", value: thinkingLevelDisplayName)
            if let priority = task.priority {
                LabeledContent("优先级", value: "P\(priority)")
            }
            LabeledContent("最近运行", value: lastRunDisplayName)
            if !task.tags.isEmpty {
                LabeledContent("标签", value: task.tags.joined(separator: "、"))
            }
            if let createdAt = task.createdAt {
                LabeledContent("创建时间", value: createdAt.formatted(date: .abbreviated, time: .standard))
            }
            if let updatedAt = task.updatedAt {
                LabeledContent("更新时间", value: updatedAt.formatted(date: .abbreviated, time: .standard))
            }
        }
        .appFont(.callout)
        .appTextSelection(allowsTextSelection)
    }

    @ViewBuilder
    private var prerequisites: some View {
        if prerequisiteItems.isEmpty {
            Text("无前置任务")
                .appFont(.callout)
                .foregroundStyle(.secondary)
        } else {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(prerequisiteItems) { item in
                    VStack(alignment: .leading, spacing: 5) {
                        HStack(spacing: 8) {
                            Text(item.title ?? "任务名称暂不可用")
                                .appFont(.callout.weight(.medium))
                            Spacer(minLength: 8)
                            if let status = item.status {
                                Text(taskStatusTitle(status))
                                    .appFont(.caption2.weight(.semibold))
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Text(item.id)
                            .appFont(.caption2.monospaced())
                            .foregroundStyle(.tertiary)
                            .appTextSelection(allowsTextSelection)
                    }
                    .padding(10)
                    .background(.quaternary.opacity(0.45), in: RoundedRectangle(cornerRadius: 9))
                }
            }
        }
    }

    private var sourceInformation: some View {
        VStack(alignment: .leading, spacing: 8) {
            LabeledContent("会话 ID", value: task.sourceSessionID ?? "-")
            LabeledContent("轮次 ID", value: task.sourceTurnID ?? "-")
            LabeledContent("源消息 ID", value: task.sourceUserMessageID ?? "-")
            LabeledContent("父任务", value: referenceDisplay(task.parentTask, fallbackID: task.parentTaskID))
            LabeledContent("来源运行", value: runDisplay(task.sourceRun, fallbackID: task.sourceRunID))
            if let projectTaskID = task.projectTaskID {
                LabeledContent("项目任务 ID", value: projectTaskID)
            }
            if let clientRef = task.executionClientRef {
                LabeledContent("执行客户端", value: clientRef)
            }
            if !task.dependencyContextRefs.isEmpty {
                LabeledContent("依赖上下文", value: task.dependencyContextRefs.joined(separator: "、"))
            }
        }
        .appFont(.callout)
        .appTextSelection(allowsTextSelection)
    }

    private var modelOutput: String? {
        task.lastRun?.reportContent?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty
    }

    private var distinctResultSummary: String? {
        // The task-level summary is updated after post-processing. During a retry it
        // may briefly still contain the previous run's terminal result, so never
        // present it as the active run's summary.
        guard !isActiveTask else { return nil }
        guard let summary = task.resultSummary?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty else {
            return nil
        }
        guard summary != modelOutput else { return nil }
        return summary
    }

    private var isActiveTask: Bool {
        ["queued", "running", "reviewing_followup", "waiting_confirmation"]
            .contains(task.normalizedStatus)
    }

    private var executionPendingMessage: String {
        switch task.normalizedStatus {
        case "reviewing_followup": model.localized("后续执行已暂停，正在审查追加要求。", english: "Further execution is paused while the follow-up is reviewed.")
        case "waiting_confirmation": model.localized("后续执行已暂停，等待你确认要求。", english: "Further execution is paused awaiting your confirmation.")
        default: model.localized("本次运行仍在执行，模型输出将在完成后显示。", english: "This run is still executing. Model output will appear when it finishes.")
        }
    }

    private var creatorDisplayName: String {
        task.creatorDisplayName?.nonEmpty
            ?? task.creatorUsername?.nonEmpty
            ?? task.creatorUserID?.nonEmpty
            ?? "-"
    }

    private var modelDisplayName: String {
        if let config = task.defaultModelConfig {
            let name = config.displayName
            if !name.isEmpty { return name }
        }
        return task.defaultModelConfigID?.nonEmpty ?? "-"
    }

    private var thinkingLevelDisplayName: String {
        task.thinkingLevel?.nonEmpty ?? "默认"
    }

    private var lastRunDisplayName: String {
        runDisplay(task.lastRun, fallbackID: task.lastRunID)
    }

    private func taskStatusTitle(_ status: String?) -> String {
        switch status?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "pending", "ready", "queued": model.localized("等待执行", english: "Waiting")
        case "running", "processing", "in_progress", "doing": model.localized("执行中", english: "Running")
        case "completed", "succeeded", "success", "done": model.localized("已完成", english: "Completed")
        case "blocked": model.localized("被阻塞", english: "Blocked")
        case "failed", "error", "execution_failed": model.localized("失败", english: "Failed")
        case "acceptance_failed": model.localized("未通过验收", english: "Acceptance Not Met")
        case "evidence_incomplete": model.localized("部分完成，待核验", english: "Partially Complete")
        case "cancelled", "canceled", "stopped": model.localized("已取消", english: "Cancelled")
        case "reviewing_followup": model.localized("审查追加要求", english: "Reviewing Follow-up")
        case "waiting_confirmation": model.localized("等待确认", english: "Awaiting Confirmation")
        case .none, "": model.localized("未知", english: "Unknown")
        default: status ?? model.localized("未知", english: "Unknown")
        }
    }

    private var prerequisiteItems: [MessageTaskReference] {
        let references = task.prerequisiteTasks.reduce(into: [String: MessageTaskReference]()) {
            $0[$1.id] = $1
        }
        var seen = Set<String>()
        return (task.prerequisiteTaskIDs + task.prerequisiteTasks.map(\.id)).compactMap { id in
            guard seen.insert(id).inserted else { return nil }
            return references[id] ?? MessageTaskReference(id: id)
        }
    }

    private func referenceDisplay(_ reference: MessageTaskReference?, fallbackID: String?) -> String {
        guard let reference else { return fallbackID?.nonEmpty ?? "-" }
        return [reference.title?.nonEmpty, reference.status?.nonEmpty, reference.id.nonEmpty]
            .compactMap { $0 }
            .joined(separator: " · ")
    }

    private func runDisplay(_ run: MessageTaskLastRunSummary?, fallbackID: String?) -> String {
        guard let run else { return fallbackID?.nonEmpty ?? "-" }
        var values = [run.status?.nonEmpty]
        if let date = run.finishedAt ?? run.startedAt {
            values.append(date.formatted(date: .abbreviated, time: .shortened))
        }
        values.append(run.id.nonEmpty)
        return values.compactMap { $0 }.joined(separator: " · ")
    }
}

struct TaskDetailTextCard: View {
    let title: String
    let text: String
    var allowsTextSelection = true

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).appFont(.subheadline.weight(.semibold))
            TaskDetailMarkdownText(
                text: text,
                allowsTextSelection: allowsTextSelection
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 10))
    }
}

private struct TaskDetailMarkdownText: View {
    let text: String
    let allowsTextSelection: Bool

    var body: some View {
        MarkdownDocumentView(
            markdown: text,
            allowsTextSelection: allowsTextSelection
        )
    }
}

private struct TaskDetailNamedText: View {
    let label: String
    let value: String
    let allowsTextSelection: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(label)
                .appFont(.caption.weight(.medium))
                .foregroundStyle(.secondary)
            MarkdownDocumentView(
                markdown: value,
                allowsTextSelection: allowsTextSelection
            )
        }
    }
}

private struct TaskDetailCodeBlock: View {
    let text: String
    let allowsTextSelection: Bool

    var body: some View {
        Text(text)
            .appFont(.caption.monospaced())
            .lineSpacing(2)
            .appTextSelection(allowsTextSelection)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(10)
            .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
    }
}

private struct TaskDetailDisclosureSection<Content: View>: View {
    let title: String
    @State private var isExpanded: Bool
    private let content: Content

    init(
        title: String,
        defaultExpanded: Bool = false,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        _isExpanded = State(initialValue: defaultExpanded)
        self.content = content()
    }

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            content
                .padding(.top, 10)
        } label: {
            Text(title).appFont(.subheadline.weight(.semibold))
        }
        .padding(12)
        .background(.quaternary.opacity(0.24), in: RoundedRectangle(cornerRadius: 10))
    }
}

private struct TaskDetailSubsection<Content: View>: View {
    let title: String
    private let content: Content

    init(title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title)
                .appFont(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
