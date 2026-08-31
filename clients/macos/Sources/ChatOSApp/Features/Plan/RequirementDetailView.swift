import ChatOSCore
import SwiftUI

struct RequirementDetailView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: ProjectPlanViewModel
    let onPreviewScope: (ProjectRequirement) -> Void
    let onOpenExecution: (ProjectRequirement) -> Void
    let onStartExecution: (ProjectRequirement) -> Void

    var body: some View {
        Group {
            if let requirement = viewModel.selectedRequirement {
                VStack(spacing: 0) {
                    header(requirement)
                    PlanDetailTabBar(viewModel: viewModel)
                    Divider()
                    detailContent(requirement)
                        .workspaceFill(alignment: .topLeading)
                }
                .overlay {
                    if viewModel.isLoadingSelection {
                        ProgressView()
                            .controlSize(.large)
                            .padding(18)
                            .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 12))
                    }
                }
            } else {
                ContentUnavailableView("选择一个需求", systemImage: "list.bullet.clipboard")
            }
        }
        .workspaceFill()
    }

    private func header(_ requirement: ProjectRequirement) -> some View {
        HStack(alignment: .top, spacing: 18) {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 7) {
                    PlanStatusBadge(status: requirement.status)
                    StatusCapsule(title: requirementTypeTitle(requirement.type), color: .secondary)
                    StatusCapsule(title: "P\(requirement.priority)", color: .orange)
                }
                Text(requirement.title)
                    .appFont(.title2.weight(.semibold))
                    .textSelection(.enabled)
                if let updatedAt = requirement.updatedAt {
                    Text("更新于 \(updatedAt.formatted(date: .abbreviated, time: .shortened))")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Spacer(minLength: 16)

            HStack(spacing: 9) {
                Button("预览范围", systemImage: "arrow.triangle.branch") {
                    onPreviewScope(requirement)
                }

                if viewModel.execution != nil {
                    Button(
                        viewModel.execution?.hasStartedRuns == true
                            ? model.localized("查看执行过程", english: "View Execution Process")
                            : model.localized("查看执行计划", english: "View Execution Plan"),
                        systemImage: "eye"
                    ) {
                        onOpenExecution(requirement)
                    }
                    .buttonStyle(.borderedProminent)
                } else if canCreateExecution(requirement) {
                    Button("打开执行工作台", systemImage: "play.fill") {
                        onStartExecution(requirement)
                    }
                    .buttonStyle(.borderedProminent)
                }
            }
        }
        .padding(.horizontal, 22)
        .padding(.top, 20)
        .padding(.bottom, 14)
    }

    @ViewBuilder
    private func detailContent(_ requirement: ProjectRequirement) -> some View {
        switch viewModel.selectedSection {
        case .requirement:
            requirementContent(requirement)
        case .documents:
            ProjectPlanDocumentsView(viewModel: viewModel)
        case .tasks:
            ProjectPlanTasksView(viewModel: viewModel)
        }
    }

    private func requirementContent(_ requirement: ProjectRequirement) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                RequirementRelationsCard(viewModel: viewModel)
                RequirementMarkdownSection(title: "摘要", value: requirement.summary)
                RequirementMarkdownSection(title: "详细说明", value: requirement.detail)
                RequirementMarkdownSection(title: "业务价值", value: requirement.businessValue)
                RequirementMarkdownSection(title: "验收标准", value: requirement.acceptanceCriteria)

                if [requirement.summary, requirement.detail, requirement.businessValue, requirement.acceptanceCriteria]
                    .allSatisfy({ ($0 ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }) {
                    ContentUnavailableView(
                        "这个需求还没有补充内容",
                        systemImage: "doc.text.magnifyingglass"
                    )
                    .frame(minHeight: 160)
                }
            }
            .padding(22)
            .frame(maxWidth: 980, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .center)
        }
    }

    private func canCreateExecution(_ requirement: ProjectRequirement) -> Bool {
        let status = requirement.status.lowercased()
        return !["done", "completed", "succeeded", "success", "cancelled", "archived"]
            .contains(status)
    }

    private func requirementTypeTitle(_ type: String) -> String {
        switch type.lowercased() {
        case "bug_fix", "bug": "缺陷修复"
        case "change": "变更"
        default: "需求"
        }
    }
}

private struct PlanDetailTabBar: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: ProjectPlanViewModel

    var body: some View {
        HStack(spacing: 2) {
            tab(.requirement, icon: "list.clipboard", count: nil)
            tab(.documents, icon: "doc.text", count: viewModel.documents.count)
            tab(.tasks, icon: "checklist", count: viewModel.workItems.count)
            Spacer()
        }
        .padding(.horizontal, 22)
    }

    private func tab(
        _ section: ProjectPlanViewModel.DetailSection,
        icon: String,
        count: Int?
    ) -> some View {
        Button {
            viewModel.selectedSection = section
        } label: {
            HStack(spacing: 6) {
                Image(systemName: icon)
                Text(section.title(language: model.interfaceLanguage))
                if let count {
                    Text("\(count)")
                        .appFont(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 1)
                        .background(.quaternary, in: Capsule())
                }
            }
            .appFont(.subheadline.weight(viewModel.selectedSection == section ? .semibold : .regular))
            .foregroundStyle(viewModel.selectedSection == section ? Color.primary : Color.secondary)
            .padding(.horizontal, 11)
            .frame(height: 42)
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(viewModel.selectedSection == section ? AppPalette.ai : Color.clear)
                    .frame(height: 2)
            }
        }
        .buttonStyle(.plain)
    }
}

private struct RequirementRelationsCard: View {
    @ObservedObject var viewModel: ProjectPlanViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("需求关系", systemImage: "link")
                .appFont(.subheadline.weight(.semibold))
            relation("前置需求", values: viewModel.selectedPrerequisites, color: .orange, empty: "无")
            relation("后续需求", values: viewModel.selectedDependents, color: AppPalette.ai, empty: "无")
            relation("子需求", values: viewModel.selectedChildren, color: AppPalette.ai, empty: "无")
            relation("执行会包含", values: viewModel.selectedExecutionScope, color: .blue, empty: "仅当前需求")
        }
        .padding(15)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 11))
        .overlay { RoundedRectangle(cornerRadius: 11).stroke(AppPalette.border.opacity(0.8)) }
    }

    private func relation(
        _ title: String,
        values: [ProjectRequirement],
        color: Color,
        empty: String
    ) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Text(title)
                .appFont(.caption.weight(.medium))
                .foregroundStyle(.secondary)
                .frame(width: 72, alignment: .leading)
            if values.isEmpty {
                Text(empty)
                    .appFont(.caption)
                    .foregroundStyle(.tertiary)
            } else {
                FlowLayout(spacing: 6) {
                    ForEach(values.prefix(12)) { requirement in
                        Text(requirement.title)
                            .appFont(.caption)
                            .foregroundStyle(color)
                            .lineLimit(1)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(color.opacity(0.09), in: RoundedRectangle(cornerRadius: 5))
                    }
                    if values.count > 12 {
                        Text("+\(values.count - 12)")
                            .appFont(.caption.weight(.medium))
                            .foregroundStyle(color)
                    }
                }
            }
        }
    }
}

private struct RequirementMarkdownSection: View {
    let title: String
    let value: String?

    var body: some View {
        if let value, !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            VStack(alignment: .leading, spacing: 9) {
                Text(title)
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                MarkdownDocumentView(markdown: value)
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 10))
                    .overlay { RoundedRectangle(cornerRadius: 10).stroke(AppPalette.border.opacity(0.72)) }
            }
        }
    }
}

struct FlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) -> CGSize {
        arrange(proposal: proposal, subviews: subviews).size
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) {
        let result = arrange(proposal: ProposedViewSize(width: bounds.width, height: proposal.height), subviews: subviews)
        for (index, point) in result.points.enumerated() {
            subviews[index].place(at: CGPoint(x: bounds.minX + point.x, y: bounds.minY + point.y), proposal: .unspecified)
        }
    }

    private func arrange(proposal: ProposedViewSize, subviews: Subviews) -> (size: CGSize, points: [CGPoint]) {
        let width = proposal.width ?? .infinity
        var points: [CGPoint] = []
        var x: CGFloat = 0
        var y: CGFloat = 0
        var lineHeight: CGFloat = 0
        var maxWidth: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                x = 0
                y += lineHeight + spacing
                lineHeight = 0
            }
            points.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            lineHeight = max(lineHeight, size.height)
            maxWidth = max(maxWidth, x - spacing)
        }
        return (CGSize(width: min(maxWidth, width), height: y + lineHeight), points)
    }
}
