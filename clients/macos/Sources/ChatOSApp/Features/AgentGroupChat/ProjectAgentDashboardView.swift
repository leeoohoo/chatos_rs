import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import SwiftUI

struct ProjectAgentDashboardView: View {
    let room: ProjectAgentRoom?
    let dashboard: LocalAgentProjectDashboard?
    let todos: [LocalAgentTodo]
    let surveys: [LocalAgentRequirementSurvey]
    let runs: [LocalAgentGroupChatRun]
    let deliveriesByRunID: [UUID: ProjectAgentDelivery]
    let profilesByID: [String: LocalAgentProfile]
    let pendingApprovalCount: Int
    let onOpen: (AgentTeamSection) -> Void
    let onOpenTodo: (LocalAgentTodo) -> Void
    let onInspectRun: (LocalAgentGroupChatRun) -> Void

    @State private var milestonePage = 0
    @State private var milestonePageSize = 5

    private var needsReviewRuns: [LocalAgentGroupChatRun] {
        runs.filter { $0.checkpoint.status == .needsReview }
    }

    private var pendingSurveys: [LocalAgentRequirementSurvey] {
        surveys.filter { $0.status == .pending }
    }

    private var blockedTodos: [LocalAgentTodo] {
        todos.filter { $0.status == .blocked }
    }

    private var humanIssues: [LocalAgentProjectIssue] {
        dashboard?.issues.filter { $0.owner == .human } ?? []
    }

    private var effectiveHealth: LocalAgentProjectHealth {
        if !needsReviewRuns.isEmpty || !blockedTodos.isEmpty {
            return dashboard?.health == .blocked ? .blocked : .atRisk
        }
        if dashboard?.health == .completed,
           todos.contains(where: { !$0.status.isTerminal }) {
            return .atRisk
        }
        return dashboard?.health ?? .onTrack
    }

    private var currentMilestone: LocalAgentProjectMilestone? {
        dashboard?.milestones.first(where: { $0.status == .inProgress })
            ?? dashboard?.milestones.first(where: { $0.status == .pending })
    }

    private var attentionCount: Int {
        needsReviewRuns.count + blockedTodos.count + pendingSurveys.count
            + pendingApprovalCount + humanIssues.count
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 14) {
                summaryPanel
                HStack(alignment: .top, spacing: 14) {
                    attentionPanel
                    executionPanel
                }
                HStack(alignment: .top, spacing: 14) {
                    milestonesPanel
                    managerBriefPanel
                }
            }
            .padding(18)
        }
        .background(AppPalette.canvas)
    }

    private var summaryPanel: some View {
        HStack(alignment: .center, spacing: 24) {
            VStack(alignment: .leading, spacing: 6) {
                Text("当前阶段")
                    .appFont(.caption2)
                    .foregroundStyle(.secondary)
                HStack(spacing: 8) {
                    Text(dashboard?.phase ?? "等待项目经理建立总览")
                        .appFont(.title3.weight(.semibold))
                    healthBadge(effectiveHealth)
                }
                Text(room?.draft.goal.isEmpty == false ? room?.draft.goal ?? "" : "项目目标尚未填写")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            VStack(alignment: .leading, spacing: 7) {
                HStack {
                    Text(currentMilestone?.title ?? "尚未建立里程碑")
                        .appFont(.caption.weight(.medium))
                    Spacer()
                    Text("\(currentMilestone?.progressPercent ?? 0)%")
                        .appFont(.caption.monospacedDigit().weight(.semibold))
                }
                ProgressView(value: Double(currentMilestone?.progressPercent ?? 0), total: 100)
                    .tint(AppPalette.ai)
                Text(updateTimeLabel)
                    .appFont(.caption2)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: 300)

            HStack(spacing: 18) {
                summaryMetric(todos.filter { $0.status == .inProgress }.count, "进行中", .blue)
                summaryMetric(todos.filter { $0.status == .pending }.count, "等待", .secondary)
                summaryMetric(blockedTodos.count, "阻塞", .orange)
                summaryMetric(attentionCount, "需你处理", .red)
            }
        }
        .dashboardPanel()
    }

    private var attentionPanel: some View {
        VStack(alignment: .leading, spacing: 10) {
            dashboardSectionHeader("需要你处理", subtitle: "系统事实与项目经理请求", count: attentionCount)
            if attentionCount == 0 {
                Label("当前没有需要你处理的事项", systemImage: "checkmark.circle.fill")
                    .appFont(.body)
                    .foregroundStyle(.green)
                    .padding(.vertical, 18)
            }
            ForEach(needsReviewRuns.prefix(3), id: \.id) { run in
                attentionRow(
                    title: "检查中断的 Agent Run",
                    detail: "\(profilesByID[run.context.agentID]?.draft.name ?? "Agent") · 写入或副作用结果需要确认",
                    icon: "exclamationmark.triangle.fill",
                    color: .orange,
                    action: { onInspectRun(run) }
                )
            }
            ForEach(blockedTodos.prefix(3)) { todo in
                attentionRow(
                    title: "处理阻塞：\(todo.title)",
                    detail: todo.blockedReason.isEmpty
                        ? "任务处于阻塞状态，但未记录具体原因"
                        : todo.blockedReason,
                    icon: "exclamationmark.octagon.fill",
                    color: .red,
                    action: { onOpenTodo(todo) }
                )
            }
            if blockedTodos.count > 3 {
                Button("另有 \(blockedTodos.count - 3) 项阻塞任务，查看全部") { onOpen(.tasks) }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
            }
            if !pendingSurveys.isEmpty {
                attentionRow(
                    title: "填写需求调研",
                    detail: "\(pendingSurveys.count) 张调研单等待你的选择",
                    icon: "list.clipboard.fill",
                    color: AppPalette.ai,
                    action: { onOpen(.research) }
                )
            }
            if pendingApprovalCount > 0 {
                attentionRow(
                    title: "审批 Agent 提案",
                    detail: "\(pendingApprovalCount) 项成员或团队变更等待确认",
                    icon: "person.crop.circle.badge.checkmark",
                    color: .blue,
                    action: { onOpen(.chat) }
                )
            }
            ForEach(humanIssues.prefix(3)) { issue in
                attentionRow(
                    title: issue.title,
                    detail: issue.requestedAction,
                    icon: issue.severity == .critical ? "exclamationmark.octagon.fill" : "person.fill.questionmark",
                    color: issue.severity == .critical ? .red : .orange,
                    action: { onOpen(issue.relatedTodoID == nil ? .chat : .tasks) }
                )
            }
        }
        .dashboardPanel()
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    private var executionPanel: some View {
        VStack(alignment: .leading, spacing: 12) {
            dashboardSectionHeader("当前执行", subtitle: "任务状态由系统实时汇总")
            statusBar
            ForEach(todos.filter { !$0.status.isTerminal }.prefix(4)) { todo in
                HStack(spacing: 10) {
                    Circle()
                        .fill(todoColor(todo.status))
                        .frame(width: 8, height: 8)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(todo.title).appFont(.body.weight(.medium)).lineLimit(1)
                        Text("\(profilesByID[todo.agentID]?.draft.name ?? "Agent") · \(todoStatusLabel(todo.status))")
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                }
            }
            Button("查看完整任务板") { onOpen(.tasks) }
                .buttonStyle(.bordered)
                .controlSize(.small)
        }
        .dashboardPanel()
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    private var milestonesPanel: some View {
        let milestones = dashboard?.milestones ?? []
        return VStack(alignment: .leading, spacing: 10) {
            dashboardSectionHeader("里程碑", subtitle: "由项目经理维护，任务事实由系统校验")
            if milestones.isEmpty {
                Text("项目经理尚未建立里程碑。")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.vertical, 18)
            }
            ForEach(milestones.agentPage(index: milestonePage, size: milestonePageSize)) { milestone in
                HStack(spacing: 10) {
                    Circle()
                        .fill(milestoneColor(milestone.status))
                        .frame(width: 9, height: 9)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(milestone.title).appFont(.body.weight(.medium))
                        Text(milestone.detail.isEmpty ? milestoneStatusLabel(milestone.status) : milestone.detail)
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                    Spacer()
                    Text("\(milestone.progressPercent)%")
                        .appFont(.caption.monospacedDigit().weight(.medium))
                }
                .padding(.vertical, 4)
            }
            if !milestones.isEmpty {
                AgentListPaginationBar(
                    totalCount: milestones.count,
                    page: $milestonePage,
                    pageSize: $milestonePageSize,
                    pageSizeOptions: [5, 10, 20]
                )
            }
        }
        .dashboardPanel()
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    private var managerBriefPanel: some View {
        VStack(alignment: .leading, spacing: 12) {
            dashboardSectionHeader(
                "项目经理简报",
                subtitle: dashboard.map { "第 \($0.revision) 版" } ?? "尚未维护"
            )
            Text(dashboard?.summary ?? "项目经理会在收到任务状态、调研和审批变化后维护阶段判断、风险与下一步。")
                .appFont(.body)
                .foregroundStyle(dashboard == nil ? .secondary : .primary)
                .textSelection(.enabled)
            if let steps = dashboard?.nextSteps, !steps.isEmpty {
                Divider()
                Text("下一步").appFont(.caption.weight(.semibold))
                ForEach(Array(steps.prefix(5).enumerated()), id: \.offset) { index, step in
                    Text("\(index + 1). \(step)")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            HStack {
                Spacer()
                Button("查看运行依据") { onOpen(.runs) }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
            }
        }
        .dashboardPanel()
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    private var statusBar: some View {
        GeometryReader { geometry in
            let total = max(1, todos.filter { $0.status != .cancelled }.count)
            HStack(spacing: 2) {
                statusSegment(.green, count: todos.filter { $0.status == .completed }.count, total: total, width: geometry.size.width)
                statusSegment(.blue, count: todos.filter { $0.status == .inProgress }.count, total: total, width: geometry.size.width)
                statusSegment(.orange, count: blockedTodos.count, total: total, width: geometry.size.width)
                statusSegment(.gray.opacity(0.28), count: todos.filter { $0.status == .pending }.count, total: total, width: geometry.size.width)
            }
        }
        .frame(height: 8)
        .clipShape(Capsule())
    }

    private var updateTimeLabel: String {
        guard let timestamp = dashboard?.updatedAtUnixMs else { return "等待项目经理首次维护" }
        let value = Date(timeIntervalSince1970: Double(timestamp) / 1_000)
            .formatted(date: .abbreviated, time: .shortened)
        return "项目经理更新于 \(value)"
    }

    private func summaryMetric(_ value: Int, _ label: String, _ color: Color) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("\(value)").appFont(.title3.weight(.semibold)).foregroundStyle(color)
            Text(label).appFont(.caption2).foregroundStyle(.secondary)
        }
    }

    private func dashboardSectionHeader(_ title: String, subtitle: String, count: Int? = nil) -> some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).appFont(.headline.weight(.semibold))
                Text(subtitle).appFont(.caption2).foregroundStyle(.secondary)
            }
            Spacer()
            if let count {
                Text("\(count)")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(count > 0 ? .orange : .secondary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background((count > 0 ? Color.orange : Color.secondary).opacity(0.1), in: Capsule())
            }
        }
    }

    private func attentionRow(
        title: String,
        detail: String,
        icon: String,
        color: Color,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: icon).foregroundStyle(color).frame(width: 22)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).appFont(.body.weight(.medium)).foregroundStyle(.primary)
                    Text(detail).appFont(.caption2).foregroundStyle(.secondary).lineLimit(2)
                }
                Spacer()
                Image(systemName: "chevron.right").appFont(.caption2).foregroundStyle(.tertiary)
            }
            .padding(9)
            .background(color.opacity(0.065), in: RoundedRectangle(cornerRadius: 9))
        }
        .buttonStyle(.plain)
    }

    private func healthBadge(_ health: LocalAgentProjectHealth) -> some View {
        let value: (String, Color) = switch health {
        case .onTrack: ("正常", .green)
        case .atRisk: ("有风险", .orange)
        case .blocked: ("阻塞", .red)
        case .completed: ("已完成", .green)
        }
        return Text(value.0)
            .appFont(.caption2.weight(.semibold))
            .foregroundStyle(value.1)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(value.1.opacity(0.1), in: Capsule())
    }

    private func statusSegment(_ color: Color, count: Int, total: Int, width: CGFloat) -> some View {
        color.frame(width: max(0, width * CGFloat(count) / CGFloat(total)))
    }

    private func todoColor(_ status: LocalAgentTodoStatus) -> Color {
        switch status {
        case .pending: .secondary
        case .inProgress: .blue
        case .blocked: .orange
        case .completed: .green
        case .cancelled: .gray
        }
    }

    private func todoStatusLabel(_ status: LocalAgentTodoStatus) -> String {
        switch status {
        case .pending: "等待"
        case .inProgress: "执行中"
        case .blocked: "阻塞"
        case .completed: "已完成"
        case .cancelled: "已取消"
        }
    }

    private func milestoneColor(_ status: LocalAgentProjectMilestoneStatus) -> Color {
        switch status {
        case .pending: .secondary
        case .inProgress: AppPalette.ai
        case .blocked: .orange
        case .completed: .green
        }
    }

    private func milestoneStatusLabel(_ status: LocalAgentProjectMilestoneStatus) -> String {
        switch status {
        case .pending: "尚未开始"
        case .inProgress: "进行中"
        case .blocked: "阻塞"
        case .completed: "已完成"
        }
    }
}

private extension View {
    func dashboardPanel() -> some View {
        padding(16)
            .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 14))
            .overlay {
                RoundedRectangle(cornerRadius: 14)
                    .stroke(AppPalette.border.opacity(0.8), lineWidth: 1)
            }
    }
}
