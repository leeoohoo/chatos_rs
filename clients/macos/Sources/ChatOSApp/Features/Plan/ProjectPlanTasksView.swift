import ChatOSCore
import SwiftUI

struct ProjectPlanTasksView: View {
    @ObservedObject var viewModel: ProjectPlanViewModel

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("项目任务").appFont(.headline)
                        Text("\(viewModel.workItems.count) 个任务 · \(viewModel.openWorkItemCount) 个未完成")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if !viewModel.workItems.isEmpty && viewModel.openWorkItemCount == 0 {
                        Label("已全部完成", systemImage: "checkmark.circle.fill")
                            .appFont(.caption.weight(.medium))
                            .foregroundStyle(.green)
                    }
                }

                if !viewModel.workItems.isEmpty {
                    Label(
                        "任务已按前置关系排序；每一项会显示它开始前必须完成的任务。",
                        systemImage: "arrow.right"
                    )
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AppPalette.surfaceSubtle, in: RoundedRectangle(cornerRadius: 9))
                }

                if viewModel.workItems.isEmpty {
                    ContentUnavailableView(
                        "这个需求下面还没有任务",
                        systemImage: "checklist"
                    )
                    .frame(minHeight: 220)
                } else {
                    ForEach(Array(viewModel.visibleWorkItems.enumerated()), id: \.element.id) { index, item in
                        ProjectWorkItemRow(
                            index: index + 1,
                            item: item,
                            prerequisites: prerequisites(for: item),
                            dependents: dependents(for: item)
                        )
                    }

                    if viewModel.hiddenWorkItemCount > 0 {
                        Button("加载更多任务（剩余 \(viewModel.hiddenWorkItemCount)）") {
                            viewModel.loadMoreWorkItems()
                        }
                        .frame(maxWidth: .infinity)
                        .buttonStyle(.bordered)
                    }
                }
            }
            .padding(22)
            .frame(maxWidth: 980, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .center)
        }
    }

    private func prerequisites(for item: ProjectWorkItem) -> [ProjectWorkItem] {
        let ids = Set(viewModel.edges.filter { $0.targetID == item.id }.map(\.sourceID))
        return viewModel.workItems.filter { ids.contains($0.id) }
    }

    private func dependents(for item: ProjectWorkItem) -> [ProjectWorkItem] {
        let ids = Set(viewModel.edges.filter { $0.sourceID == item.id }.map(\.targetID))
        return viewModel.workItems.filter { ids.contains($0.id) }
    }
}

private struct ProjectWorkItemRow: View {
    let index: Int
    let item: ProjectWorkItem
    let prerequisites: [ProjectWorkItem]
    let dependents: [ProjectWorkItem]

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            HStack(alignment: .top, spacing: 11) {
                Text("\(index)")
                    .appFont(.caption.monospacedDigit().weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 24, height: 24)
                    .background(.quaternary, in: Circle())
                VStack(alignment: .leading, spacing: 5) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(item.title).appFont(.subheadline.weight(.semibold))
                        Spacer()
                        WorkItemStatusBadge(status: item.status)
                        StatusCapsule(title: "P\(item.priority)", color: .orange)
                    }
                    if let detail = item.detail, !detail.isEmpty {
                        MarkdownDocumentView(markdown: detail)
                    }
                }
            }

            if !prerequisites.isEmpty || !dependents.isEmpty {
                VStack(alignment: .leading, spacing: 7) {
                    dependencyLine("前置项目任务", values: prerequisites, color: .orange, empty: "无")
                    if !dependents.isEmpty {
                        dependencyLine("后续项目任务", values: dependents, color: AppPalette.ai, empty: "无")
                    }
                }
                .padding(.leading, 35)
                .padding(.top, 3)
            }

            if !item.tags.isEmpty || item.dueAt != nil {
                FlowLayout(spacing: 6) {
                    ForEach(item.tags, id: \.self) { tag in
                        PlanTaskMetadataPill(title: tag)
                    }
                    if let dueAt = item.dueAt {
                        PlanTaskMetadataPill(
                            title: "截止 \(dueAt.formatted(date: .abbreviated, time: .shortened))"
                        )
                    }
                }
                .padding(.leading, 35)
            }
        }
        .padding(14)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 11))
        .overlay { RoundedRectangle(cornerRadius: 11).stroke(AppPalette.border.opacity(0.75)) }
    }

    private func dependencyLine(
        _ title: String,
        values: [ProjectWorkItem],
        color: Color,
        empty: String
    ) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text(title)
                .appFont(.caption.weight(.medium))
                .foregroundStyle(.secondary)
                .frame(width: 88, alignment: .leading)
            if values.isEmpty {
                Text(empty).appFont(.caption).foregroundStyle(.tertiary)
            } else {
                FlowLayout(spacing: 5) {
                    ForEach(values) { value in
                        Text(value.title)
                            .appFont(.caption)
                            .foregroundStyle(color)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(color.opacity(0.09), in: RoundedRectangle(cornerRadius: 5))
                    }
                }
            }
        }
    }
}

private struct PlanTaskMetadataPill: View {
    let title: String

    var body: some View {
        Text(title)
            .appFont(.caption2)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(.quaternary, in: RoundedRectangle(cornerRadius: 5))
    }
}

private struct WorkItemStatusBadge: View {
    let status: String

    var body: some View {
        StatusCapsule(title: title, color: color)
    }

    private var title: String {
        switch status.lowercased() {
        case "done", "completed", "succeeded", "success": "已完成"
        case "in_progress", "running", "processing": "进行中"
        case "blocked": "阻塞"
        case "failed", "error": "失败"
        case "ready", "todo", "pending": "待处理"
        default: status
        }
    }

    private var color: Color {
        switch status.lowercased() {
        case "done", "completed", "succeeded", "success": .green
        case "in_progress", "running", "processing": AppPalette.ai
        case "blocked": .orange
        case "failed", "error": .red
        default: .secondary
        }
    }
}
