import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import SwiftUI

struct TeamTodoBoardView: View {
    let todos: [LocalAgentTodo]
    let profilesByID: [String: LocalAgentProfile]

    @State private var expandedTodoIDs: Set<String> = []
    @State private var resultTodo: LocalAgentTodo?

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                if todos.isEmpty {
                    ContentUnavailableView(
                        "还没有团队任务",
                        systemImage: "checklist",
                        description: Text("项目经理创建的任务会显示在这里。")
                    )
                    .padding(.top, 70)
                }
                ForEach(todos) { todo in
                    todoCard(todo)
                }
            }
            .padding(18)
        }
        .sheet(item: $resultTodo) { todo in
            TeamTodoResultDetailView(
                todo: todo,
                profile: profilesByID[todo.agentID]
            )
        }
    }

    private func todoCard(_ todo: LocalAgentTodo) -> some View {
        let profile = profilesByID[todo.agentID]
        let agentName = profile?.draft.name ?? "Agent"
        let isExpanded = expandedTodoIDs.contains(todo.id)
        let authorizedToolCount = todo.executionPlan.builtinCapabilities.reduce(0) {
            $0 + LocalAgentTodoAuthorizationCatalog.descriptor(for: $1).toolNames.count
        }

        return VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .top, spacing: 12) {
                AgentAvatarView(
                    name: agentName,
                    data: profile?.draft.avatarData,
                    size: AgentAvatarMetrics.navigation,
                    cornerRadius: 15
                )

                VStack(alignment: .leading, spacing: 7) {
                    HStack(alignment: .firstTextBaseline, spacing: 10) {
                        Text(todo.title)
                            .appFont(.headline)
                            .lineLimit(2)
                        Spacer(minLength: 12)
                        statusBadge(todo.status)
                    }

                    Label("负责人 · \(agentName)", systemImage: "person.crop.circle.fill")
                        .appFont(.caption.weight(.medium))
                        .foregroundStyle(.secondary)

                    Text(todo.executionContract.objective)
                        .appFont(.body)
                        .foregroundStyle(.primary)
                        .lineLimit(isExpanded ? nil : 2)
                        .textSelection(.enabled)
                }
            }

            if !todo.blockedReason.isEmpty {
                Label(todo.blockedReason, systemImage: "exclamationmark.octagon.fill")
                    .appFont(.caption)
                    .foregroundStyle(.orange)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.orange.opacity(0.10), in: RoundedRectangle(cornerRadius: 8))
            }

            HStack(spacing: 8) {
                if !todo.executionContract.expectedOutputs.isEmpty {
                    metadataBadge(
                        "\(todo.executionContract.expectedOutputs.count) 项交付",
                        systemImage: "shippingbox"
                    )
                }
                if !todo.executionContract.acceptanceCriteria.isEmpty {
                    metadataBadge(
                        "\(todo.executionContract.acceptanceCriteria.count) 项验收",
                        systemImage: "checkmark.seal"
                    )
                }
                if authorizedToolCount > 0 {
                    metadataBadge(
                        "\(authorizedToolCount) 个工具",
                        systemImage: "wrench.and.screwdriver"
                    )
                }
                if !todo.executionPlan.plugins.isEmpty {
                    metadataBadge(
                        "\(todo.executionPlan.plugins.count) 个插件",
                        systemImage: "puzzlepiece.extension"
                    )
                }
                if !todo.result.isEmpty {
                    metadataBadge("已有执行结果", systemImage: "doc.text.fill", color: .green)
                }
                Spacer(minLength: 8)
                Button {
                    if isExpanded {
                        expandedTodoIDs.remove(todo.id)
                    } else {
                        expandedTodoIDs.insert(todo.id)
                    }
                } label: {
                    Label(isExpanded ? "收起" : "查看详情", systemImage: isExpanded ? "chevron.up" : "chevron.down")
                        .appFont(.caption.weight(.semibold))
                }
                .buttonStyle(.plain)
                .foregroundStyle(AppPalette.ai)
            }

            if isExpanded {
                Divider()

                VStack(alignment: .leading, spacing: 10) {
                    contractText(
                        "范围",
                        systemImage: "scope",
                        value: todo.executionContract.scope
                    )
                    contractList(
                        "交付物",
                        systemImage: "shippingbox.fill",
                        values: todo.executionContract.expectedOutputs
                    )
                    contractList(
                        "验收条件",
                        systemImage: "checkmark.seal.fill",
                        values: todo.executionContract.acceptanceCriteria
                    )
                    contractList(
                        "约束",
                        systemImage: "lock.fill",
                        values: todo.executionContract.constraints
                    )
                    authorizationPanel(todo.executionPlan)

                    if !todo.result.isEmpty {
                        resultSummary(todo)
                    }
                }
            }
        }
        .padding(16)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .stroke(AppPalette.border.opacity(0.85), lineWidth: 1)
        }
        .overlay(alignment: .leading) {
            Capsule()
                .fill(todoStatusColor(todo.status))
                .frame(width: 4)
                .padding(.vertical, 14)
                .padding(.leading, 2)
        }
        .shadow(color: .black.opacity(0.035), radius: 3, y: 1)
    }

    private func authorizationPanel(_ plan: LocalAgentTodoExecutionPlan) -> some View {
        let descriptors = plan.builtinCapabilities.map {
            LocalAgentTodoAuthorizationCatalog.descriptor(for: $0)
        }
        return VStack(alignment: .leading, spacing: 12) {
            sectionLabel("任务授权", systemImage: "key.fill")

            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text("执行模式")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                Text(plan.requiresExecution ? "独立执行环境" : "仅协调，无独立执行")
                    .appFont(.caption)
                Spacer(minLength: 8)
            }

            if descriptors.isEmpty {
                authorizationEmptyState("未授权项目文件或终端工具。")
            } else {
                VStack(alignment: .leading, spacing: 10) {
                    Text("内置工具")
                        .appFont(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ForEach(descriptors) { descriptor in
                        authorizationCapability(descriptor)
                    }
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("插件")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                if plan.plugins.isEmpty {
                    authorizationEmptyState("未授权额外插件。")
                } else {
                    ForEach(Array(plan.plugins.enumerated()), id: \.element.pluginID) { _, plugin in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(plugin.displayName)
                                .appFont(.caption.weight(.semibold))
                            Text(plugin.pluginID)
                                .appFont(.caption2.monospaced())
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                            if !plugin.reason.isEmpty {
                                Text("授权理由：\(plugin.reason)")
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                                    .textSelection(.enabled)
                            }
                        }
                        .padding(10)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 8))
                    }
                    Text("插件的具体工具在执行时通过 capability_search → capability_describe 按需加载，并由 capability_invoke 调用。")
                        .appFont(.caption2)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }

            HStack(spacing: 6) {
                Text("授权快照：\(plan.selectionRevision)")
                if plan.selectedAtUnixMs > 0 {
                    Text("·")
                    Text(authorizationDate(plan.selectedAtUnixMs))
                }
            }
            .appFont(.caption2.monospaced())
            .foregroundStyle(.tertiary)
            .textSelection(.enabled)

            Text("这里展示任务创建时锁定的执行授权；Agent 职业与项目规则不属于本 Todo 单独授权。")
                .appFont(.caption2)
                .foregroundStyle(.secondary)
        }
        .contractPanel(tint: AppPalette.ai.opacity(0.045))
    }

    private func authorizationCapability(
        _ descriptor: LocalAgentTodoBuiltinAuthorizationDescriptor
    ) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 7) {
                Text(descriptor.displayName)
                    .appFont(.caption.weight(.semibold))
                Text(descriptor.capability.rawValue)
                    .appFont(.caption2.monospaced())
                    .foregroundStyle(AppPalette.ai)
            }
            Text(descriptor.detail)
                .appFont(.caption2)
                .foregroundStyle(.secondary)
            Text(descriptor.toolNames.joined(separator: "  ·  "))
                .appFont(.caption2.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 8))
    }

    private func authorizationEmptyState(_ text: String) -> some View {
        Text(text)
            .appFont(.caption)
            .foregroundStyle(.secondary)
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 8))
    }

    private func authorizationDate(_ unixMilliseconds: Int64) -> String {
        Date(timeIntervalSince1970: TimeInterval(unixMilliseconds) / 1_000)
            .formatted(date: .abbreviated, time: .standard)
    }

    private func statusBadge(_ status: LocalAgentTodoStatus) -> some View {
        Label(todoStatusLabel(status), systemImage: todoStatusIcon(status))
            .appFont(.caption2.weight(.semibold))
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(todoStatusColor(status).opacity(0.13), in: Capsule())
            .foregroundStyle(todoStatusColor(status))
    }

    private func metadataBadge(
        _ title: String,
        systemImage: String,
        color: Color = .secondary
    ) -> some View {
        Label(title, systemImage: systemImage)
            .appFont(.caption2.weight(.medium))
            .foregroundStyle(color)
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(AppPalette.surfaceSubtle, in: Capsule())
    }

    @ViewBuilder
    private func contractText(_ title: String, systemImage: String, value: String) -> some View {
        if !value.isEmpty {
            VStack(alignment: .leading, spacing: 7) {
                sectionLabel(title, systemImage: systemImage)
                Text(value)
                    .appFont(.caption)
                    .textSelection(.enabled)
            }
            .contractPanel()
        }
    }

    @ViewBuilder
    private func contractList(_ title: String, systemImage: String, values: [String]) -> some View {
        if !values.isEmpty {
            VStack(alignment: .leading, spacing: 7) {
                sectionLabel(title, systemImage: systemImage)
                ForEach(Array(values.enumerated()), id: \.offset) { _, value in
                    HStack(alignment: .firstTextBaseline, spacing: 7) {
                        Circle()
                            .fill(AppPalette.ai.opacity(0.65))
                            .frame(width: 4, height: 4)
                        Text(value)
                            .appFont(.caption)
                            .textSelection(.enabled)
                    }
                }
            }
            .contractPanel()
        }
    }

    private func resultSummary(_ todo: LocalAgentTodo) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "checkmark.circle.fill")
                .font(.title3)
                .foregroundStyle(.green)

            VStack(alignment: .leading, spacing: 6) {
                Text("执行结果")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(.green)
                Text(todo.result)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }

            Spacer(minLength: 12)

            Button("查看完整结果") {
                resultTodo = todo
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .contractPanel(tint: Color.green.opacity(0.055))
    }

    private func sectionLabel(
        _ title: String,
        systemImage: String,
        color: Color = AppPalette.ai
    ) -> some View {
        Label(title, systemImage: systemImage)
            .appFont(.caption.weight(.semibold))
            .foregroundStyle(color)
    }

    private func todoStatusLabel(_ status: LocalAgentTodoStatus) -> String {
        switch status {
        case .pending: "待执行"
        case .inProgress: "执行中"
        case .blocked: "已阻塞"
        case .completed: "已完成"
        case .cancelled: "已取消"
        }
    }

    private func todoStatusColor(_ status: LocalAgentTodoStatus) -> Color {
        switch status {
        case .pending: .secondary
        case .inProgress: .blue
        case .blocked: .orange
        case .completed: .green
        case .cancelled: .red
        }
    }

    private func todoStatusIcon(_ status: LocalAgentTodoStatus) -> String {
        switch status {
        case .pending: "clock"
        case .inProgress: "arrow.trianglehead.2.clockwise.rotate.90"
        case .blocked: "exclamationmark.octagon.fill"
        case .completed: "checkmark.circle.fill"
        case .cancelled: "xmark.circle.fill"
        }
    }
}

private struct TeamTodoResultDetailView: View {
    @Environment(\.dismiss) private var dismiss

    let todo: LocalAgentTodo
    let profile: LocalAgentProfile?

    private var agentName: String { profile?.draft.name ?? "Agent" }

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                AgentAvatarView(
                    name: agentName,
                    data: profile?.draft.avatarData,
                    size: AgentAvatarMetrics.navigation,
                    cornerRadius: 15
                )

                VStack(alignment: .leading, spacing: 5) {
                    Text(todo.title)
                        .appFont(.headline)
                        .lineLimit(2)
                    Label("负责人 · \(agentName)", systemImage: "person.crop.circle.fill")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }

                Spacer(minLength: 16)

                Button("完成") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(18)

            Divider()

            VStack(alignment: .leading, spacing: 14) {
                Label("执行结果", systemImage: "doc.text.fill")
                    .appFont(.subheadline.weight(.semibold))
                    .foregroundStyle(.green)
                MarkdownReaderView(markdown: todo.result)
            }
            .padding(20)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        }
        .frame(minWidth: 760, idealWidth: 920, minHeight: 560, idealHeight: 700)
        .background(AppPalette.canvas)
    }
}

private extension View {
    func contractPanel(tint: Color = AppPalette.surfaceSubtle) -> some View {
        padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(tint, in: RoundedRectangle(cornerRadius: 10))
    }
}
