import ChatOSCore
import SwiftUI

struct RequirementBrowserView: View {
    @ObservedObject var viewModel: ProjectPlanViewModel

    var body: some View {
        VStack(spacing: 0) {
            stats
            Divider()
            ScrollView(.horizontal) {
                LazyHStack(spacing: 0) {
                    ForEach(viewModel.requirementColumns) { column in
                        requirementColumn(column)
                            .frame(width: 320)
                    }
                }
            }
            .scrollIndicators(.visible)
            .workspaceFill(alignment: .topLeading)
        }
        .background(AppPalette.canvas)
        .workspaceFill()
    }

    private var stats: some View {
        HStack(spacing: 8) {
            PlanStat(title: "需求", value: viewModel.requirements.count, color: AppPalette.ai)
            PlanStat(title: "完成", value: viewModel.snapshot?.counts.done ?? 0, color: .green)
            PlanStat(title: "阻塞", value: viewModel.snapshot?.counts.blocked ?? 0, color: .orange)
        }
        .padding(10)
    }

    private func requirementColumn(_ column: RequirementColumn) -> some View {
        VStack(spacing: 0) {
            HStack(spacing: 7) {
                if column.id == "root" {
                    Image(systemName: "arrow.triangle.branch")
                        .foregroundStyle(.secondary)
                }
                Text(column.title)
                    .appFont(.caption.weight(.semibold))
                    .lineLimit(1)
                Spacer()
                Text("\(column.requirements.count)")
                    .appFont(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(.quaternary, in: Capsule())
            }
            .padding(.horizontal, 12)
            .frame(height: 40)

            Divider()

            ScrollView {
                LazyVStack(spacing: 6) {
                    ForEach(column.requirements) { requirement in
                        requirementRow(requirement)
                    }
                }
                .padding(8)
            }
        }
        .background(AppPalette.surface)
        .overlay(alignment: .trailing) { Divider() }
        .workspaceFill(alignment: .top)
    }

    private func requirementRow(_ requirement: ProjectRequirement) -> some View {
        let selected = requirement.id == viewModel.selectedRequirementID
        let inPath = viewModel.requirementPath.contains(requirement.id)
        let childCount = viewModel.requirementChildren[requirement.id]?.count ?? 0
        let taskCount = viewModel.loadedWorkItemCount(for: requirement.id)
        let prerequisites = viewModel.prerequisites(for: requirement)
        let dependents = viewModel.dependents(for: requirement)

        return Button {
            viewModel.selectedRequirementID = requirement.id
        } label: {
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline, spacing: 7) {
                    Text(requirement.title)
                        .appFont(.subheadline.weight(.semibold))
                        .foregroundStyle(.primary)
                        .lineLimit(2)
                    Spacer(minLength: 6)
                    Text(taskCount.map(String.init) ?? "-")
                        .appFont(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(AppPalette.surface, in: Capsule())
                        .overlay { Capsule().stroke(.separator.opacity(0.55)) }
                    if childCount > 0 {
                        Image(systemName: "chevron.right")
                            .appFont(.caption2.weight(.bold))
                            .foregroundStyle(.tertiary)
                    }
                }

                if let summary = requirement.summary, !summary.isEmpty {
                    Text(summary)
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }

                FlowLayout(spacing: 6) {
                    PlanStatusBadge(status: requirement.status)
                    PlanMetadataPill(title: requirementTypeTitle(requirement.type))
                    PlanMetadataPill(title: "P\(requirement.priority)")
                    if !prerequisites.isEmpty {
                        PlanMetadataPill(title: "前置 \(prerequisites.count)", color: .orange)
                    }
                    if !dependents.isEmpty {
                        PlanMetadataPill(title: "后续 \(dependents.count)", color: AppPalette.ai)
                    }
                    if childCount > 0 {
                        PlanMetadataPill(title: "子需求 \(childCount)", color: AppPalette.ai)
                    }
                }

                relationPreview("前置", requirements: prerequisites, color: .orange)
                relationPreview("后续", requirements: dependents, color: AppPalette.ai)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                selected ? AppPalette.ai.opacity(0.12) : (inPath ? Color.accentColor.opacity(0.06) : Color.clear),
                in: RoundedRectangle(cornerRadius: 9)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 9)
                    .stroke(
                        selected
                            ? AppPalette.ai.opacity(0.55)
                            : Color(nsColor: .separatorColor).opacity(0.65)
                    )
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func relationPreview(
        _ title: String,
        requirements: [ProjectRequirement],
        color: Color
    ) -> some View {
        if !requirements.isEmpty {
            Text("\(title)：\(previewText(requirements))")
                .appFont(.caption2)
                .foregroundStyle(color)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func previewText(_ requirements: [ProjectRequirement]) -> String {
        let names = requirements.prefix(2).map(\.title).joined(separator: "、")
        return requirements.count > 2 ? "\(names) 等 \(requirements.count) 个" : names
    }

    private func requirementTypeTitle(_ type: String) -> String {
        switch type.lowercased() {
        case "bug_fix", "bug": "缺陷修复"
        case "change": "变更"
        default: "需求"
        }
    }
}

private struct PlanMetadataPill: View {
    let title: String
    var color: Color = .secondary

    var body: some View {
        Text(title)
            .appFont(.caption2)
            .foregroundStyle(color)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(color.opacity(0.08), in: Capsule())
            .overlay { Capsule().stroke(color.opacity(0.18)) }
    }
}

private struct PlanStat: View {
    let title: String
    let value: Int
    let color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(LocalizedStringKey(title)).appFont(.caption2).foregroundStyle(.secondary)
            Text("\(value)").appFont(.subheadline.weight(.semibold)).foregroundStyle(color)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 8))
        .overlay { RoundedRectangle(cornerRadius: 8).stroke(AppPalette.border.opacity(0.75)) }
    }
}

struct PlanStatusBadge: View {
    let status: String

    var body: some View {
        Text(LocalizedStringKey(title))
            .appFont(.caption2.weight(.medium))
            .foregroundStyle(color)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(color.opacity(0.10), in: Capsule())
    }

    private var title: String {
        switch status.lowercased() {
        case "done", "completed", "succeeded", "success": "完成"
        case "in_progress", "processing", "running": "进行中"
        case "blocked": "阻塞"
        case "failed", "error": "失败"
        case "approved": "已确认"
        case "reviewing": "评审中"
        case "ready": "就绪"
        case "cancelled", "canceled": "取消"
        case "archived": "归档"
        case "todo": "待办"
        case "draft", "": "草稿"
        default: status
        }
    }

    private var color: Color {
        switch status.lowercased() {
        case "done", "completed", "succeeded", "success": .green
        case "in_progress", "processing", "running", "reviewing": AppPalette.ai
        case "blocked", "failed", "error": .red
        case "approved", "ready": .orange
        default: .secondary
        }
    }
}
