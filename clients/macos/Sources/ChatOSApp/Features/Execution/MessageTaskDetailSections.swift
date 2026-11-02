import ChatOSCore
import Foundation
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
        if task.presentationStatus == .dependencyBlocked {
            TaskDetailDisclosureSection(title: "未启动原因", defaultExpanded: true) {
                VStack(alignment: .leading, spacing: 8) {
                    Text(dependencyBlockExplanation)
                    ForEach(dependencyBlockers) { prerequisite in
                        HStack(spacing: 8) {
                            Text(prerequisite.title ?? prerequisite.id)
                            Spacer(minLength: 8)
                            Text(taskStatusTitle(prerequisite.displayStatus ?? prerequisite.status))
                                .appFont(.caption2.weight(.semibold))
                                .foregroundStyle(.orange)
                        }
                    }
                    Text("请先处理对应的前置任务；其交付条件满足后，后续任务才应重新执行。")
                        .foregroundStyle(.secondary)
                }
            }
        }
        if let report = task.lastRun?.acceptanceReport {
            TaskDetailDisclosureSection(
                title: report.checks.isEmpty ? "交付结果" : "验收结果",
                defaultExpanded: true
            ) {
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
        TaskDetailDisclosureSection(
            title: "任务内容",
            systemImage: "scope",
            defaultExpanded: true
        ) {
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

        modelOutputSection

        if let summary = distinctResultSummary {
            TaskDetailDisclosureSection(title: "执行结果摘要", systemImage: "text.badge.checkmark") {
                TaskDetailMarkdownText(
                    text: summary,
                    allowsTextSelection: allowsTextSelection
                )
            }
            .id("result-summary-\(task.id)")
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

        TaskDetailDisclosureSection(title: "技术与诊断信息", systemImage: "wrench.and.screwdriver") {
            VStack(alignment: .leading, spacing: 14) {
                Label("这些信息用于排查问题，日常查看无需展开。", systemImage: "info.circle")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)

                TaskDetailSubsection(title: "基本信息") {
                    basicInformation
                }

                TaskDetailSubsection(title: "前置任务（\(prerequisiteItems.count)）") {
                    prerequisites
                }

                if let mcpConfig = task.mcpConfigJSON {
                    TaskDetailDisclosureSection(title: "MCP / 工作区 / 服务器", systemImage: "server.rack") {
                        TaskDetailCodeBlock(
                            text: mcpConfig,
                            allowsTextSelection: allowsTextSelection
                        )
                    }
                }

                if let toolState = task.taskToolStateJSON {
                    TaskDetailDisclosureSection(title: "调用摘要原始数据", systemImage: "shippingbox") {
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
                    TaskDetailDisclosureSection(title: "调度配置", systemImage: "calendar.badge.clock") {
                        TaskDetailCodeBlock(
                            text: schedule,
                            allowsTextSelection: allowsTextSelection
                        )
                    }
                }

                if let input = task.inputPayloadJSON {
                    TaskDetailDisclosureSection(title: "原始输入", systemImage: "curlybraces.square") {
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
            HStack(spacing: 10) {
                Image(systemName: "sparkles")
                    .appFont(.system(size: 13, weight: .semibold))
                    .foregroundStyle(AppPalette.ai)
                    .frame(width: 28, height: 28)
                    .background(AppPalette.aiSoft, in: RoundedRectangle(cornerRadius: 8))
                VStack(alignment: .leading, spacing: 2) {
                    Text("交付输出").appFont(.subheadline.weight(.semibold))
                    Text(isActiveTask ? "任务完成后将在这里呈现最终结果" : "本次任务返回的主要内容")
                        .appFont(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

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
        .background(
            LinearGradient(
                colors: [AppPalette.ai.opacity(0.075), AppPalette.surface],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            ),
            in: RoundedRectangle(cornerRadius: 12)
        )
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(AppPalette.ai.opacity(0.18), lineWidth: 1)
        }
    }

    private var basicInformation: some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 132), spacing: 10)],
            alignment: .leading,
            spacing: 10
        ) {
            TaskDetailFact(
                label: "状态",
                value: MessageTaskStatusPresentation.title(
                    for: task,
                    localized: { model.localized($0, english: $1) }
                ),
                systemImage: "circle.fill"
            )
            TaskDetailFact(label: "执行模型", value: modelDisplayName, systemImage: "cpu")
            TaskDetailFact(label: "思考等级", value: thinkingLevelDisplayName, systemImage: "brain")
            TaskDetailFact(label: "创建人", value: creatorDisplayName, systemImage: "person")
            if let priority = task.priority {
                TaskDetailFact(label: "优先级", value: "P\(priority)", systemImage: "flag")
            }
            if !task.tags.isEmpty {
                TaskDetailFact(label: "标签", value: task.tags.joined(separator: "、"), systemImage: "tag")
            }
            if let createdAt = task.createdAt {
                TaskDetailFact(
                    label: "创建时间",
                    value: createdAt.formatted(date: .abbreviated, time: .shortened),
                    systemImage: "calendar"
                )
            }
            if let updatedAt = task.updatedAt {
                TaskDetailFact(
                    label: "最近更新",
                    value: updatedAt.formatted(date: .abbreviated, time: .shortened),
                    systemImage: "clock.arrow.circlepath"
                )
            }
        }
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
                                Text(taskStatusTitle(item.displayStatus ?? status))
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
            LabeledContent("任务 ID", value: task.id)
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
            LabeledContent("最近运行", value: lastRunDisplayName)
        }
        .appFont(.caption.monospaced())
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
        MessageTaskStatusPresentation.title(
            forRawStatus: status,
            localized: { model.localized($0, english: $1) }
        )
    }

    private var dependencyBlockers: [MessageTaskReference] {
        let blockers = task.dependencyBlockingPrerequisites
        return blockers.isEmpty ? prerequisiteItems : blockers
    }

    private var dependencyBlockExplanation: String {
        let names = dependencyBlockers.prefix(3).map { $0.title ?? $0.id }.joined(separator: "、")
        guard !names.isEmpty else {
            return model.localized(
                "前置任务的交付条件未满足，因此本任务没有启动。",
                english: "A prerequisite delivery condition was not met, so this task did not start."
            )
        }
        return model.localized(
            "前置任务「\(names)」的交付条件未满足，因此本任务没有启动。",
            english: "Prerequisite \(names) did not meet its delivery condition, so this task did not start."
        )
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
        return [reference.title?.nonEmpty, reference.displayStatus?.nonEmpty ?? reference.status?.nonEmpty, reference.id.nonEmpty]
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
        ScrollView([.horizontal, .vertical]) {
            Text(formattedText)
                .appFont(.caption.monospaced())
                .lineSpacing(3)
                .appTextSelection(allowsTextSelection)
                .fixedSize(horizontal: true, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
        }
        .frame(minHeight: 90, maxHeight: 240)
        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
        .overlay { RoundedRectangle(cornerRadius: 9).stroke(AppPalette.border.opacity(0.65)) }
    }

    private var formattedText: String {
        guard let data = text.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data),
              JSONSerialization.isValidJSONObject(object),
              let pretty = try? JSONSerialization.data(
                  withJSONObject: object,
                  options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
              ),
              let result = String(data: pretty, encoding: .utf8) else { return text }
        return result
    }
}

private struct TaskDetailDisclosureSection<Content: View>: View {
    let title: String
    let systemImage: String?
    @State private var isExpanded: Bool
    private let content: Content

    init(
        title: String,
        systemImage: String? = nil,
        defaultExpanded: Bool = false,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.systemImage = systemImage
        _isExpanded = State(initialValue: defaultExpanded)
        self.content = content()
    }

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            content
                .padding(.top, 10)
        } label: {
            HStack(spacing: 8) {
                if let systemImage {
                    Image(systemName: systemImage)
                        .foregroundStyle(AppPalette.ai)
                        .frame(width: 18)
                }
                Text(title).appFont(.subheadline.weight(.semibold))
            }
        }
        .padding(13)
        .background(AppPalette.surfaceSubtle.opacity(0.48), in: RoundedRectangle(cornerRadius: 11))
        .overlay { RoundedRectangle(cornerRadius: 11).stroke(AppPalette.border.opacity(0.55)) }
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

private struct TaskDetailFact: View {
    let label: String
    let value: String
    let systemImage: String

    var body: some View {
        HStack(alignment: .top, spacing: 9) {
            Image(systemName: systemImage)
                .appFont(.caption.weight(.semibold))
                .foregroundStyle(AppPalette.ai)
                .frame(width: 17, height: 17)
            VStack(alignment: .leading, spacing: 3) {
                Text(label)
                    .appFont(.caption2)
                    .foregroundStyle(.secondary)
                Text(value)
                    .appFont(.caption.weight(.medium))
                    .lineLimit(2)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 42, alignment: .topLeading)
        .padding(10)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 9))
        .overlay { RoundedRectangle(cornerRadius: 9).stroke(AppPalette.border.opacity(0.45)) }
    }
}

struct TaskCapabilityUsageSummary: Decodable {
    struct Plugin: Decodable, Identifiable {
        let id: String
        let key: String
        let componentKey: String?
        let displayName: String
        let callCount: Int
        let status: String

        private enum CodingKeys: String, CodingKey {
            case id, key, status
            case componentKey = "component_key"
            case displayName = "display_name"
            case callCount = "call_count"
        }
    }

    struct Tool: Decodable, Identifiable {
        var id: String { "\(pluginID ?? "builtin"):\(name):\(status)" }
        let name: String
        let displayName: String
        let route: String
        let pluginID: String?
        let pluginName: String?
        let status: String
        let callCount: Int
        let latestAtUnixMs: Double

        private enum CodingKeys: String, CodingKey {
            case name, route, status
            case displayName = "display_name"
            case pluginID = "plugin_id"
            case pluginName = "plugin_name"
            case callCount = "call_count"
            case latestAtUnixMs = "latest_at_unix_ms"
        }
    }

    let plugins: [Plugin]
    let tools: [Tool]

    static func decode(_ json: String?) -> Self? {
        guard let json,
              let data = json.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(Self.self, from: data)
    }
}

struct TaskCapabilityUsageView: View {
    private struct UsedPlugin: Identifiable {
        let id: String
        let name: String
        let callCount: Int
    }

    let usage: TaskCapabilityUsageSummary
    @State private var showsSystemTools = false
    @State private var showsUnusedPlugins = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 10) {
                Image(systemName: "puzzlepiece.extension.fill")
                    .appFont(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 30, height: 30)
                    .background(AppPalette.ai, in: RoundedRectangle(cornerRadius: 9))
                VStack(alignment: .leading, spacing: 2) {
                    Text("插件与工具").appFont(.subheadline.weight(.semibold))
                    Text("只在这里汇总本任务真实使用的能力")
                        .appFont(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                StatusCapsule(title: "\(totalCallCount) 次调用", color: AppPalette.ai)
            }

            HStack(spacing: 8) {
                usageFact(value: usedPlugins.count, label: "实际插件", systemImage: "puzzlepiece.extension")
                usageFact(value: externalTools.count, label: "外部工具", systemImage: "point.3.connected.trianglepath.dotted")
                usageFact(value: systemTools.count, label: "系统工具", systemImage: "gearshape.2")
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("实际使用的插件")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                if usedPlugins.isEmpty {
                    Label("本任务没有调用插件能力", systemImage: "minus.circle")
                        .appFont(.callout)
                        .foregroundStyle(.secondary)
                        .padding(.vertical, 6)
                } else {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 180), spacing: 8)],
                        alignment: .leading,
                        spacing: 8
                    ) {
                        ForEach(usedPlugins) { plugin in
                            usedPluginCard(plugin)
                        }
                    }
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("插件与外部工具调用")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                if externalTools.isEmpty {
                    Label("尚未产生插件或外部工具调用", systemImage: "hourglass")
                        .appFont(.callout)
                        .foregroundStyle(.secondary)
                        .padding(.vertical, 6)
                } else {
                    ForEach(externalTools) { tool in
                        toolRow(tool)
                    }
                }
            }

            if !systemTools.isEmpty {
                DisclosureGroup(isExpanded: $showsSystemTools) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("系统工具用于读取文件、搜索和记录过程，默认折叠以减少干扰。")
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                            .padding(.bottom, 4)
                        ForEach(systemTools) { tool in
                            toolRow(tool)
                        }
                    }
                    .padding(.top, 8)
                } label: {
                    HStack {
                        Text("系统内置工具")
                            .appFont(.caption.weight(.semibold))
                        Spacer()
                        Text("\(systemCallCount) 次")
                            .appFont(.caption2.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                }
            }

            if !unusedPlugins.isEmpty {
                DisclosureGroup(isExpanded: $showsUnusedPlugins) {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(unusedPlugins) { plugin in
                            HStack {
                                Text(plugin.displayName)
                                    .appFont(.caption)
                                Spacer()
                                Text("未调用")
                                    .appFont(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    .padding(.top, 8)
                } label: {
                    Text("已启用但未调用（\(unusedPlugins.count)）")
                        .appFont(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(14)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 12))
        .overlay { RoundedRectangle(cornerRadius: 12).stroke(AppPalette.ai.opacity(0.22)) }
    }

    private var totalCallCount: Int {
        usage.tools.reduce(0) { $0 + $1.callCount }
    }

    private var systemCallCount: Int {
        systemTools.reduce(0) { $0 + $1.callCount }
    }

    private var externalTools: [TaskCapabilityUsageSummary.Tool] {
        usage.tools.filter(isExternalTool)
    }

    private var systemTools: [TaskCapabilityUsageSummary.Tool] {
        usage.tools.filter { !isExternalTool($0) }
    }

    private var usedPlugins: [UsedPlugin] {
        var totals: [String: Int] = [:]
        for tool in externalTools where resolvedPluginName(for: tool) != nil {
            guard let name = resolvedPluginName(for: tool) else { continue }
            totals[name, default: 0] += tool.callCount
        }
        return totals.map { UsedPlugin(id: $0.key, name: $0.key, callCount: $0.value) }
            .sorted { lhs, rhs in
                lhs.callCount == rhs.callCount ? lhs.name < rhs.name : lhs.callCount > rhs.callCount
            }
    }

    private var unusedPlugins: [TaskCapabilityUsageSummary.Plugin] {
        let names = Set(usedPlugins.map(\.name))
        return usage.plugins.filter { !names.contains($0.displayName) }
    }

    private func usageFact(value: Int, label: String, systemImage: String) -> some View {
        HStack(spacing: 7) {
            Image(systemName: systemImage)
                .appFont(.caption)
                .foregroundStyle(AppPalette.ai)
            VStack(alignment: .leading, spacing: 1) {
                Text(String(value)).appFont(.callout.weight(.semibold)).monospacedDigit()
                Text(label).appFont(.caption2).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(9)
        .background(AppPalette.surfaceSubtle.opacity(0.7), in: RoundedRectangle(cornerRadius: 9))
    }

    private func usedPluginCard(_ plugin: UsedPlugin) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "puzzlepiece.extension.fill")
                .foregroundStyle(AppPalette.ai)
                .frame(width: 26, height: 26)
                .background(AppPalette.ai.opacity(0.10), in: RoundedRectangle(cornerRadius: 7))
            VStack(alignment: .leading, spacing: 2) {
                Text(plugin.name).appFont(.caption.weight(.semibold)).lineLimit(1)
                Text("实际调用 \(plugin.callCount) 次")
                    .appFont(.caption2)
                    .foregroundStyle(AppPalette.ai)
            }
            Spacer(minLength: 0)
        }
        .padding(9)
        .background(AppPalette.surfaceSubtle.opacity(0.6), in: RoundedRectangle(cornerRadius: 9))
    }

    private func toolRow(_ tool: TaskCapabilityUsageSummary.Tool) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: toolIcon(tool.status))
                .appFont(.caption.weight(.semibold))
                .foregroundStyle(toolColor(tool.status))
                .frame(width: 26, height: 26)
                .background(toolColor(tool.status).opacity(0.10), in: Circle())
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(tool.displayName).appFont(.callout.weight(.medium)).lineLimit(1)
                    if tool.callCount > 1 {
                        Text("×\(tool.callCount)").appFont(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                    }
                }
                Text(resolvedPluginName(for: tool).map { "插件 · \($0)" } ?? routeTitle(tool.route))
                    .appFont(.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            Text(toolStatus(tool.status))
                .appFont(.caption2.weight(.semibold))
                .foregroundStyle(toolColor(tool.status))
                .padding(.horizontal, 7)
                .padding(.vertical, 3)
                .background(toolColor(tool.status).opacity(0.09), in: Capsule())
        }
        .padding(.vertical, 5)
    }

    private func isExternalTool(_ tool: TaskCapabilityUsageSummary.Tool) -> Bool {
        if tool.pluginID?.nonEmpty != nil || tool.pluginName?.nonEmpty != nil { return true }
        return ["external_mcp", "mcp", "plugin", "capability", "project"]
            .contains(tool.route.lowercased())
    }

    private func resolvedPluginName(for tool: TaskCapabilityUsageSummary.Tool) -> String? {
        let candidates = [tool.pluginID, tool.pluginName]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty }
        if let plugin = usage.plugins.first(where: { plugin in
            let identifiers = [plugin.id, plugin.key, plugin.componentKey, plugin.displayName]
                .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty }
            return !Set(identifiers).isDisjoint(with: candidates)
        }) {
            return plugin.displayName
        }
        return tool.pluginName?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty
    }

    private func toolStatus(_ status: String) -> String {
        switch status.lowercased() {
        case "succeeded", "completed": "成功"
        case "failed": "失败"
        case "needs_review": "待确认"
        case "cancelled", "canceled": "已取消"
        default: "进行中"
        }
    }

    private func routeTitle(_ route: String) -> String {
        switch route.lowercased() {
        case "external_mcp", "mcp": "外部 MCP 工具"
        case "plugin", "capability": "插件能力"
        case "project": "项目工具"
        default: "系统内置工具"
        }
    }

    private func toolIcon(_ status: String) -> String {
        switch status.lowercased() {
        case "succeeded", "completed": "checkmark"
        case "failed", "cancelled", "canceled": "exclamationmark"
        case "needs_review": "questionmark"
        default: "arrow.triangle.2.circlepath"
        }
    }

    private func toolColor(_ status: String) -> Color {
        switch status.lowercased() {
        case "succeeded", "completed": .green
        case "failed", "cancelled", "canceled": .red
        case "needs_review": .orange
        default: AppPalette.ai
        }
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
