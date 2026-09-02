import ChatOSCore
import SwiftUI

struct MessageTaskDetailSections: View {
    let task: MessageTask
    var isLoadingModelOutput = false
    var allowsTextSelection = true

    var body: some View {
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
            LabeledContent("状态", value: task.status ?? "未知")
            LabeledContent("创建人", value: creatorDisplayName)
            LabeledContent("模型", value: modelDisplayName)
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
                                Text(status)
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
        guard let summary = task.resultSummary?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty else {
            return nil
        }
        guard summary != modelOutput else { return nil }
        return summary
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

    private var lastRunDisplayName: String {
        runDisplay(task.lastRun, fallbackID: task.lastRunID)
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
