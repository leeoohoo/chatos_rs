import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import SwiftUI

struct TeamRunsView: View {
    let runs: [LocalAgentGroupChatRun]
    let profilesByID: [String: LocalAgentProfile]
    let deliveriesByRunID: [UUID: ProjectAgentDelivery]
    let selectedAgentID: String?
    let onSelectAgent: (String?) -> Void
    let onInspect: (LocalAgentGroupChatRun) -> Void

    private var visibleRuns: [LocalAgentGroupChatRun] {
        guard let selectedAgentID else { return runs }
        return runs.filter { $0.context.agentID == selectedAgentID }
    }

    private var selectedAgentName: String? {
        guard let selectedAgentID else { return nil }
        return profilesByID[selectedAgentID]?.draft.name ?? "Agent"
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(selectedAgentName.map { "\($0) 的运行" } ?? "团队全部运行")
                        .appFont(.headline)
                    Text(selectedAgentID == nil
                         ? "按更新时间查看团队内所有 Run"
                         : "通讯与任务执行彼此独立，可同时运行")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if selectedAgentID != nil {
                    Button("查看全部") { onSelectAgent(nil) }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                }
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 12)

            if selectedAgentID != nil {
                HStack(spacing: 12) {
                    laneSummary(.manager)
                    laneSummary(.executor)
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 14)
            }

            Divider()

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    if visibleRuns.isEmpty {
                        ContentUnavailableView(
                            selectedAgentID == nil ? "还没有运行记录" : "这个 Agent 还没有运行记录",
                            systemImage: "waveform.path.ecg",
                            description: Text("Agent 被唤醒后，通讯与任务执行 Run 会显示在这里。")
                        )
                        .padding(.top, 70)
                    }
                    ForEach(visibleRuns, id: \.id) { run in
                        VStack(alignment: .leading, spacing: 7) {
                            HStack {
                                Text(profilesByID[run.context.agentID]?.draft.name ?? "Agent")
                                    .appFont(.headline)
                                laneBadge(run.context.lane)
                                Spacer()
                                Text(run.checkpoint.status.displayName)
                                    .appFont(.caption.monospacedDigit())
                                    .foregroundStyle(statusColor(run.checkpoint.status))
                                Image(systemName: "chevron.right")
                                    .appFont(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                            if let delivery = deliveriesByRunID[run.id] {
                                Text("\(delivery.triggerKind.displayName) · \(run.events.count) 条事件 · \(run.checkpoint.modelCalls) 次模型调用")
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            HStack(spacing: 12) {
                                Text(Self.timestamp(run.updatedAtUnixMs))
                                    .appFont(.caption2.monospacedDigit())
                                    .foregroundStyle(.secondary)
                                if let threadID = run.checkpoint.memory?.scope.threadID {
                                    Text("Memory \(threadID)")
                                        .appFont(.caption2.monospaced())
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                }
                            }
                            if let reason = run.checkpoint.stopReason, !reason.isEmpty {
                                Text(reason).appFont(.caption).foregroundStyle(.secondary)
                            }
                        }
                        .padding(14)
                        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
                        .contentShape(Rectangle())
                        .onTapGesture { onInspect(run) }
                    }
                }
                .padding(18)
            }
        }
    }

    private func laneSummary(_ lane: LocalAgentRunLane) -> some View {
        let run = visibleRuns.first { $0.context.lane == lane }
        return VStack(alignment: .leading, spacing: 8) {
            HStack {
                laneBadge(lane)
                Spacer()
                if let run {
                    Circle()
                        .fill(statusColor(run.checkpoint.status))
                        .frame(width: 7, height: 7)
                    Text(run.checkpoint.status.displayName)
                        .appFont(.caption.weight(.medium))
                        .foregroundStyle(statusColor(run.checkpoint.status))
                } else {
                    Text("暂无记录")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            if let run {
                Text(deliveriesByRunID[run.id]?.triggerKind.displayName ?? "未知触发")
                    .appFont(.caption)
                Text("更新于 \(Self.timestamp(run.updatedAtUnixMs))")
                    .appFont(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
            } else {
                Text(lane == .manager ? "等待消息、心跳或任务状态唤醒" : "等待已启动的团队任务")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, minHeight: 82, alignment: .topLeading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
        .contentShape(Rectangle())
        .onTapGesture {
            if let run { onInspect(run) }
        }
    }

    private func laneBadge(_ lane: LocalAgentRunLane) -> some View {
        Text(lane == .manager ? "通讯" : "任务执行")
            .appFont(.caption2.weight(.semibold))
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(
                (lane == .manager ? Color.blue : Color.purple).opacity(0.14),
                in: Capsule()
            )
    }

    private func statusColor(_ status: AgentRunCheckpoint.Status) -> Color {
        switch status {
        case .running: .blue
        case .completed: .green
        case .failed: .red
        case .paused, .needsReview, .limitReached: .orange
        case .ready: .secondary
        }
    }

    private static func timestamp(_ unixMs: Int64) -> String {
        Date(timeIntervalSince1970: Double(unixMs) / 1_000)
            .formatted(date: .abbreviated, time: .standard)
    }
}

struct TeamRunInspectorSheet: View {
    @Environment(\.dismiss) private var dismiss
    let run: LocalAgentGroupChatRun
    let delivery: ProjectAgentDelivery?
    let agentName: String

    private struct ToolInspection: Identifiable {
        let id: String
        let name: String
        let arguments: String
        let status: String
        let outcome: AgentToolOutcome?
    }

    private var toolInspections: [ToolInspection] {
        run.checkpoint.messages.flatMap { message in
            message.toolCalls.map { call in
                let outcome = run.checkpoint.receipts[call.id]
                let status: String
                if call.id == run.checkpoint.inFlightCallID {
                    status = "执行中断"
                } else if run.checkpoint.pendingCalls.contains(where: { $0.id == call.id }) {
                    status = "等待执行"
                } else if outcome?.isError == true {
                    status = "失败"
                } else if outcome != nil {
                    status = "已完成"
                } else {
                    status = "未执行"
                }
                return ToolInspection(
                    id: call.id,
                    name: call.name,
                    arguments: Self.prettyJSON(call.arguments),
                    status: status,
                    outcome: outcome
                )
            }
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Run 检查器").appFont(.title3.weight(.semibold))
                    Text("\(agentName) · \(run.context.lane == .manager ? "通讯" : "任务执行")")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("完成") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            .padding(16)
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    runSummary
                    inspectorSection("工具调用") {
                        if toolInspections.isEmpty {
                            Text("本次 Run 没有工具调用。")
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(toolInspections) { tool in
                                toolInspection(tool)
                            }
                        }
                    }
                    inspectorSection("运行事件") {
                        if run.events.isEmpty {
                            Text("本次 Run 没有事件记录。")
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(run.events.reversed()) { event in
                                HStack(alignment: .top, spacing: 10) {
                                    Circle()
                                        .fill(eventColor(event.kind))
                                        .frame(width: 7, height: 7)
                                        .padding(.top, 6)
                                    VStack(alignment: .leading, spacing: 3) {
                                        HStack {
                                            Text(event.kind).appFont(.caption.weight(.semibold))
                                            Spacer()
                                            Text(event.date.formatted(date: .abbreviated, time: .standard))
                                                .appFont(.caption2.monospacedDigit())
                                                .foregroundStyle(.secondary)
                                        }
                                        Text(event.detail).appFont(.caption).textSelection(.enabled)
                                        Text("模型调用：\(event.modelCalls)")
                                            .appFont(.caption2)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                if event.id != run.events.first?.id { Divider() }
                            }
                        }
                    }
                }
                .padding(20)
            }
        }
        .frame(minWidth: 900, minHeight: 680)
    }

    private var runSummary: some View {
        inspectorSection("运行信息") {
            Grid(alignment: .leading, horizontalSpacing: 24, verticalSpacing: 9) {
                summaryRow("状态", run.checkpoint.status.displayName)
                summaryRow("触发", delivery?.triggerKind.displayName ?? "未知")
                summaryRow("Delivery", delivery?.status.displayName ?? "未知")
                summaryRow("Memory", run.context.lane == .manager ? "Agent 长期通讯" : "Todo 独立执行")
                summaryRow("模型配置", run.modelConfigID)
                summaryRow("模型调用", "\(run.checkpoint.modelCalls) / \(run.policy.maximumModelCalls)")
                summaryRow("运行耗时", String(format: "%.1f 秒", run.checkpoint.elapsedSeconds))
                summaryRow("开始时间", Self.timestamp(run.createdAtUnixMs))
                summaryRow("更新时间", Self.timestamp(run.updatedAtUnixMs))
            }
            if let reason = run.checkpoint.stopReason, !reason.isEmpty {
                Divider()
                LabeledContent("停止原因") {
                    Text(reason).textSelection(.enabled)
                }
                .appFont(.caption)
            }
            if let result = run.checkpoint.result, !result.isEmpty {
                Divider()
                VStack(alignment: .leading, spacing: 6) {
                    Text("结果").appFont(.caption.weight(.semibold))
                    MarkdownDocumentView(markdown: result)
                }
            }
        }
    }

    private func summaryRow(_ label: String, _ value: String) -> some View {
        GridRow {
            Text(label).appFont(.caption).foregroundStyle(.secondary)
            Text(value).appFont(.caption).textSelection(.enabled)
        }
    }

    private func toolInspection(_ tool: ToolInspection) -> some View {
        DisclosureGroup {
            VStack(alignment: .leading, spacing: 10) {
                Text("参数").appFont(.caption.weight(.semibold))
                Text(tool.arguments)
                    .appFont(.caption.monospaced())
                    .textSelection(.enabled)
                if let outcome = tool.outcome {
                    Text("结果").appFont(.caption.weight(.semibold))
                    Text(outcome.content)
                        .appFont(.caption.monospaced())
                        .foregroundStyle(outcome.isError ? Color.red : Color.primary)
                        .textSelection(.enabled)
                }
            }
            .padding(.top, 8)
        } label: {
            HStack {
                Text(tool.name).appFont(.body.monospaced())
                Spacer()
                Text(tool.status)
                    .appFont(.caption2.weight(.semibold))
                    .foregroundStyle(tool.status == "失败" ? Color.red : Color.secondary)
            }
        }
        .padding(12)
        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
    }

    private func inspectorSection<Content: View>(
        _ title: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).appFont(.headline)
            content()
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
    }

    private func eventColor(_ kind: String) -> Color {
        if kind.contains("failed") || kind.contains("error") || kind == "stopped" { return .red }
        if kind.contains("completed") { return .green }
        if kind.contains("pause") || kind.contains("review") { return .orange }
        if kind.contains("tool") { return .purple }
        return .blue
    }

    private static func prettyJSON(_ raw: String) -> String {
        guard let data = raw.data(using: .utf8),
              let value = try? JSONSerialization.jsonObject(with: data),
              let formatted = try? JSONSerialization.data(
                withJSONObject: value,
                options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
              ) else { return raw }
        return String(decoding: formatted, as: UTF8.self)
    }

    private static func timestamp(_ unixMs: Int64) -> String {
        Date(timeIntervalSince1970: Double(unixMs) / 1_000)
            .formatted(date: .abbreviated, time: .standard)
    }
}

extension AgentRunCheckpoint.Status {
    var displayName: String {
        switch self {
        case .ready: "等待开始"
        case .running: "运行中"
        case .paused: "已暂停"
        case .completed: "已完成"
        case .failed: "失败"
        case .needsReview: "需要检查"
        case .limitReached: "达到限制"
        }
    }
}

extension ProjectAgentDeliveryStatus {
    var displayName: String {
        switch self {
        case .pending: "等待处理"
        case .running: "处理中"
        case .completed: "已完成"
        case .failed: "失败"
        case .cancelled: "已取消"
        }
    }
}

extension ProjectAgentDeliveryTriggerKind {
    var displayName: String {
        switch self {
        case .mention: "Human @消息"
        case .defaultAgent: "Human 未点名消息"
        case .agentMention: "Agent 消息"
        case .heartbeat: "主动巡检"
        case .todo: "Todo 执行"
        case .todoStatus: "Todo 状态变化"
        }
    }
}

extension LocalAgentTeamAssetCategory {
    var displayName: String {
        switch self {
        case .overview: "项目背景"
        case .currentProgress: "整体进度"
        case .techStack: "技术栈"
        case .architecture: "架构"
        case .conventions: "工程规范"
        case .decision: "重要决策"
        case .reference: "参考资料"
        }
    }
}
