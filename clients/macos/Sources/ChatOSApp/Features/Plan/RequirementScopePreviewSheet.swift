import AppKit
import ChatOSCore
import SwiftUI

struct RequirementScopePreviewSheet: View {
    let requirement: ProjectRequirement
    @ObservedObject var viewModel: ProjectPlanViewModel
    @Environment(\.dismiss) private var dismiss
    @State private var includePrerequisiteDependents = false

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            controls
            Divider()
            RequirementScopeGraphCanvas(
                rootID: requirement.id,
                requirements: scopeRequirements,
                edges: scopeEdges,
                baseIDs: Set(baseIDs),
                downstreamIDs: Set(downstreamIDs)
            )
            .workspaceFill()
            footer
        }
        .frame(width: sheetSize.width, height: sheetSize.height)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 4) {
                Label("执行范围预览", systemImage: "arrow.triangle.branch")
                    .appFont(.title3.weight(.semibold))
                Text(requirement.title)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            Spacer()
            Button("关闭", action: dismiss.callAsFunction)
        }
        .padding(16)
    }

    private var controls: some View {
        HStack(spacing: 14) {
            ScopeLegend(title: "当前", color: .blue)
            ScopeLegend(title: "主线", color: .secondary)
            ScopeLegend(title: "补齐前置", color: .orange)
            ScopeLegend(title: "额外后续", color: .green)
            Spacer()
            PlanStatChip(title: "当前范围", value: scopeIDs.count)
            PlanStatChip(title: "默认范围", value: baseIDs.count)
            PlanStatChip(title: "额外后续", value: max(0, scopeIDs.count - baseIDs.count))
            Toggle("包含前置需求关联的额外后续需求", isOn: $includePrerequisiteDependents)
                .toggleStyle(.switch)
                .controlSize(.small)
        }
        .padding(12)
    }

    private var footer: some View {
        HStack {
            Text("这里只预览执行计划覆盖的需求范围，不会生成任务，也不会启动执行。")
                .appFont(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Button("完成", action: dismiss.callAsFunction)
                .buttonStyle(.borderedProminent)
        }
        .padding(14)
        .overlay(alignment: .top) { Divider() }
    }

    private var baseIDs: [String] {
        viewModel.executionScopeIDs(includePrerequisiteDependents: false)
    }

    private var downstreamIDs: [String] {
        viewModel.downstreamScopeIDs(rootID: requirement.id)
    }

    private var scopeIDs: [String] {
        viewModel.executionScopeIDs(includePrerequisiteDependents: includePrerequisiteDependents)
    }

    private var scopeRequirements: [ProjectRequirement] {
        let ids = Set(scopeIDs)
        return viewModel.requirements.filter { ids.contains($0.id) }
    }

    private var scopeEdges: [ProjectPlanEdge] {
        viewModel.requirementScopeEdges(scopeIDs: scopeIDs)
    }

    private var sheetSize: CGSize {
        let visible = NSScreen.main?.visibleFrame.size ?? CGSize(width: 1440, height: 900)
        return CGSize(
            width: min(1420, max(1120, visible.width - 72)),
            height: min(860, max(720, visible.height - 90))
        )
    }
}

private struct ScopeLegend: View {
    let title: String
    let color: Color

    var body: some View {
        Text(title)
            .appFont(.caption.weight(.medium))
            .foregroundStyle(color)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(color.opacity(0.09), in: Capsule())
    }
}

private struct PlanStatChip: View {
    let title: String
    let value: Int

    var body: some View {
        HStack(spacing: 5) {
            Text(title).foregroundStyle(.secondary)
            Text("\(value)").fontWeight(.semibold)
        }
        .appFont(.caption)
    }
}

private struct RequirementScopeGraphCanvas: View {
    let rootID: String
    let requirements: [ProjectRequirement]
    let edges: [ProjectPlanEdge]
    let baseIDs: Set<String>
    let downstreamIDs: Set<String>
    @State private var zoom = 1.0
    @State private var panOffset: CGSize = .zero
    @State private var positioned = false
    @GestureState private var dragTranslation: CGSize = .zero

    var body: some View {
        GeometryReader { proxy in
            let layout = RequirementScopeLayout(requirements: requirements, edges: edges)
            ZStack(alignment: .topLeading) {
                DotGridSurface()
                ZStack(alignment: .topLeading) {
                    Canvas { context, _ in
                        for edge in edges {
                            guard let source = layout.positions[edge.sourceID],
                                  let target = layout.positions[edge.targetID] else { continue }
                            let start = CGPoint(x: (source.x + 112) * zoom, y: source.y * zoom)
                            let end = CGPoint(x: (target.x - 112) * zoom, y: target.y * zoom)
                            var path = Path()
                            path.move(to: start)
                            let middle = (start.x + end.x) / 2
                            path.addCurve(
                                to: end,
                                control1: CGPoint(x: middle, y: start.y),
                                control2: CGPoint(x: middle, y: end.y)
                            )
                            context.stroke(
                                path,
                                with: .color(edge.kind == "child" ? Color.secondary.opacity(0.45) : AppPalette.ai.opacity(0.72)),
                                style: StrokeStyle(lineWidth: 1.7 * zoom, dash: edge.kind == "child" ? [6 * zoom, 5 * zoom] : [])
                            )
                        }
                    }
                    .frame(width: layout.size.width * zoom, height: layout.size.height * zoom)

                    ForEach(requirements) { requirement in
                        if let point = layout.positions[requirement.id] {
                            RequirementScopeNode(
                                requirement: requirement,
                                kind: kind(for: requirement.id),
                                zoom: zoom
                            )
                            .position(x: point.x * zoom, y: point.y * zoom)
                        }
                    }
                }
                .frame(width: layout.size.width * zoom, height: layout.size.height * zoom, alignment: .topLeading)
                .offset(
                    x: panOffset.width + dragTranslation.width,
                    y: panOffset.height + dragTranslation.height
                )
            }
            .background {
                MessageTaskGraphScrollZoomCapture { delta, location in
                    let factor = min(1.16, max(0.86, exp(Double(delta) * 0.025)))
                    setZoom(min(2, max(0.35, zoom * factor)), anchor: location)
                }
                .allowsHitTesting(false)
            }
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 3)
                    .updating($dragTranslation) { value, state, _ in state = value.translation }
                    .onEnded { value in
                        panOffset.width += value.translation.width
                        panOffset.height += value.translation.height
                    }
            )
            .overlay(alignment: .bottomTrailing) {
                HStack(spacing: 4) {
                    Button("缩小", systemImage: "minus") { setZoom(max(0.35, zoom - 0.1), container: proxy.size) }
                        .labelStyle(.iconOnly)
                    Button("\(Int(zoom * 100))%") { setZoom(1, container: proxy.size) }
                        .frame(minWidth: 54)
                    Button("放大", systemImage: "plus") { setZoom(min(2, zoom + 0.1), container: proxy.size) }
                        .labelStyle(.iconOnly)
                    Button("居中", systemImage: "scope") { center(container: proxy.size, content: layout.size) }
                }
                .controlSize(.small)
                .padding(7)
                .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 9))
                .overlay { RoundedRectangle(cornerRadius: 9).stroke(.separator) }
                .padding(12)
            }
            .clipped()
            .onAppear {
                guard !positioned else { return }
                zoom = 1
                center(container: proxy.size, content: layout.size)
                positioned = true
            }
            .onChange(of: graphSignature) {
                center(container: proxy.size, content: layout.size)
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
    }

    private var graphSignature: String {
        requirements.map(\.id).sorted().joined(separator: "|")
    }

    private func kind(for id: String) -> RequirementScopeKind {
        if id == rootID { return .current }
        if !baseIDs.contains(id) { return .optional }
        if downstreamIDs.contains(id) { return .main }
        return .prerequisite
    }

    private func center(container: CGSize, content: CGSize) {
        panOffset = CGSize(
            width: (container.width - content.width * zoom) / 2,
            height: (container.height - content.height * zoom) / 2
        )
    }

    private func setZoom(_ value: Double, container: CGSize) {
        setZoom(value, anchor: CGPoint(x: container.width / 2, y: container.height / 2))
    }

    private func setZoom(_ value: Double, anchor: CGPoint) {
        guard zoom > 0, value != zoom else { return }
        let contentPoint = CGPoint(
            x: (anchor.x - panOffset.width) / zoom,
            y: (anchor.y - panOffset.height) / zoom
        )
        zoom = value
        panOffset = CGSize(
            width: anchor.x - contentPoint.x * value,
            height: anchor.y - contentPoint.y * value
        )
    }
}

private enum RequirementScopeKind {
    case current, main, prerequisite, optional

    var title: String {
        switch self {
        case .current: "当前"
        case .main: "主线"
        case .prerequisite: "补齐前置"
        case .optional: "额外后续"
        }
    }

    var color: Color {
        switch self {
        case .current: .blue
        case .main: .secondary
        case .prerequisite: .orange
        case .optional: .green
        }
    }
}

private struct RequirementScopeNode: View {
    let requirement: ProjectRequirement
    let kind: RequirementScopeKind
    let zoom: Double

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(kind.title)
                    .foregroundStyle(kind.color)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(kind.color.opacity(0.10), in: Capsule())
                PlanStatusBadge(status: requirement.status)
            }
            .appFont(.caption2.weight(.medium))
            Text(requirement.title)
                .appFont(.caption.weight(.semibold))
                .lineLimit(2)
            if let summary = requirement.summary, !summary.isEmpty {
                Text(summary)
                    .appFont(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .padding(11 * zoom)
        .frame(width: 224 * zoom, height: 86 * zoom, alignment: .topLeading)
        .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 9 * zoom))
        .overlay {
            RoundedRectangle(cornerRadius: 9 * zoom)
                .stroke(kind.color.opacity(0.55), lineWidth: max(1, zoom))
        }
    }
}

private struct RequirementScopeLayout {
    var positions: [String: CGPoint]
    var size: CGSize

    init(requirements: [ProjectRequirement], edges: [ProjectPlanEdge]) {
        let ids = Set(requirements.map(\.id))
        let relevant = edges.filter { ids.contains($0.sourceID) && ids.contains($0.targetID) }
        let incoming = Dictionary(grouping: relevant, by: \.targetID)
        var levels: [String: Int] = [:]
        for _ in 0..<max(1, requirements.count) {
            var changed = false
            for requirement in requirements {
                let level = (incoming[requirement.id] ?? [])
                    .map { (levels[$0.sourceID] ?? 0) + 1 }
                    .max() ?? 0
                if levels[requirement.id] != level {
                    levels[requirement.id] = level
                    changed = true
                }
            }
            if !changed { break }
        }
        let grouped = Dictionary(grouping: requirements, by: { levels[$0.id] ?? 0 })
        var result: [String: CGPoint] = [:]
        for (level, items) in grouped {
            for (row, requirement) in items.enumerated() {
                result[requirement.id] = CGPoint(
                    x: CGFloat(level) * 320 + 150,
                    y: CGFloat(row) * 122 + 74
                )
            }
        }
        positions = result
        let maxLevel = grouped.keys.max() ?? 0
        let maxRows = grouped.values.map(\.count).max() ?? 1
        size = CGSize(
            width: CGFloat(maxLevel + 1) * 320 + 44,
            height: CGFloat(maxRows) * 122 + 28
        )
    }
}
