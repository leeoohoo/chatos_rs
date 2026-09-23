import ChatOSCore
import SwiftUI

struct StoryRelationGraphView: View {
    @EnvironmentObject private var appModel: AppModel
    let project: StoryProject
    let selectedSegmentID: String?
    let openAsset: (String) -> Void
    let openSegment: (String) -> Void

    @State private var showsWholeStory = false
    @State private var zoom: CGFloat = 1
    @State private var hoveredNodeID: String?
    @State private var selectedRelationID: String?
    @State private var hoveredRelationID: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            header
            if project.segments.isEmpty && project.resources.isEmpty {
                ContentUnavailableView(
                    appModel.localized("尚无关系数据", english: "No Relationship Data"),
                    systemImage: "point.3.connected.trianglepath.dotted",
                    description: Text(appModel.localized(
                        "完成全剧规划后，这里会显示分段引用和人物在场景中的关系。",
                        english: "After story planning, segment references and character-in-scene relationships appear here."
                    ))
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                if !visibleRelations.isEmpty { relationStrip }
                graphCanvas
            }
        }
    }

    private var header: some View {
        HStack(alignment: .center, spacing: 16) {
            ZStack {
                RoundedRectangle(cornerRadius: 11, style: .continuous).fill(Color.teal.opacity(0.11))
                Image(systemName: "point.3.connected.trianglepath.dotted")
                    .font(.system(size: 19, weight: .semibold))
                    .foregroundStyle(.teal)
            }
            .frame(width: 44, height: 44)

            VStack(alignment: .leading, spacing: 4) {
                Text(appModel.localized("关系图谱", english: "Relationship Map")).font(.title2.bold())
                Text(appModel.localized(
                    "从分段出发查看角色、场景与道具引用，悬停节点即可追踪关系。",
                    english: "Trace characters, scenes and props from each segment. Hover any node to follow its connections."
                ))
                .font(.callout)
                .foregroundStyle(.secondary)
            }

            Spacer(minLength: 16)
            graphMetric(project.segments.count, appModel.localized("分段", english: "Segments"), color: .blue)
            graphMetric(project.resources.count, appModel.localized("素材", english: "Assets"), color: .orange)
            graphMetric(project.relations.count, appModel.localized("动作", english: "Actions"), color: .purple)
            Divider().frame(height: 30)
            Toggle(appModel.localized("显示全剧", english: "Full Story"), isOn: $showsWholeStory)
                .toggleStyle(.switch)
                .controlSize(.small)
                .disabled(selectedSegmentID == nil)
            Divider().frame(height: 30)
            HStack(spacing: 7) {
                Button { zoom = max(0.7, zoom - 0.1) } label: { Image(systemName: "minus.magnifyingglass") }
                Text("\(Int(zoom * 100))%")
                    .font(.caption.monospacedDigit())
                    .frame(width: 42)
                Button { zoom = min(1.35, zoom + 0.1) } label: { Image(systemName: "plus.magnifyingglass") }
                Button("100%") { zoom = 1 }
            }
            .buttonStyle(.borderless)
        }
        .padding(20)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Color.teal.opacity(0.14)))
    }

    private var graphCanvas: some View {
        ScrollView([.horizontal, .vertical]) {
            let graph = layout
            ZStack(alignment: .topLeading) {
                ZStack(alignment: .topLeading) {
                    graphBackground(graph)
                    columnLabels(graph)
                    graphLegend
                    edges(graph)
                    ForEach(graph.nodes) { node in
                        nodeCard(node, graph: graph).position(graph.positions[node.id] ?? .zero)
                    }
                }
                .frame(width: graph.size.width, height: graph.size.height)
                .scaleEffect(zoom, anchor: .topLeading)
            }
            .frame(width: graph.size.width * zoom, height: graph.size.height * zoom, alignment: .topLeading)
        }
        .background(Color(nsColor: .textBackgroundColor).opacity(0.74),
                    in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Color.teal.opacity(0.14)))
        .shadow(color: Color.black.opacity(0.035), radius: 12, y: 4)
    }

    private var visibleSegments: [StorySegment] {
        showsWholeStory || selectedSegmentID == nil
            ? project.segments
            : project.segments.filter { $0.id == selectedSegmentID }
    }

    private var visibleRelations: [StorySegmentRelation] {
        let ids = Set(visibleSegments.map(\.id))
        return project.relations.filter { ids.contains($0.segmentID) }
    }

    private var highlightedRelationID: String? {
        guard let candidate = hoveredRelationID ?? selectedRelationID,
              visibleRelations.contains(where: { $0.id == candidate }) else { return nil }
        return candidate
    }

    private var relationStrip: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Label(appModel.localized("人物动作关系", english: "Character Actions"), systemImage: "figure.walk.motion")
                    .font(.subheadline.bold())
                    .foregroundStyle(.purple)
                Spacer()
                Text(appModel.localized("悬停或点击可聚焦关系", english: "Hover or click to focus a relationship"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 10) {
                    ForEach(Array(visibleRelations.enumerated()), id: \.element.id) { index, relation in
                        relationCard(relation, number: index + 1)
                    }
                }
            }
        }
        .padding(14)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Color.purple.opacity(0.12)))
    }

    private func relationCard(_ relation: StorySegmentRelation, number: Int) -> some View {
        let focused = highlightedRelationID == relation.id
        return Button {
            selectedRelationID = selectedRelationID == relation.id ? nil : relation.id
        } label: {
            HStack(spacing: 9) {
                Text("\(number)")
                    .font(.caption2.bold())
                    .foregroundStyle(.white)
                    .frame(width: 22, height: 22)
                    .background(Color.purple, in: Circle())
                VStack(alignment: .leading, spacing: 3) {
                    Text("\(characterName(relation.characterID)) → \(sceneName(relation.sceneID))")
                        .font(.caption.bold()).foregroundStyle(.primary).lineLimit(1)
                    Text(relation.action).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                }
                Divider().frame(height: 26)
                VStack(alignment: .leading, spacing: 2) {
                    Text(relation.position.isEmpty ? appModel.localized("场景内", english: "In scene") : relation.position)
                        .font(.caption2.weight(.medium)).foregroundStyle(.secondary).lineLimit(1)
                    Text("\(relation.startSecond)–\(relation.endSecond)s")
                        .font(.caption2.monospacedDigit()).foregroundStyle(.tertiary)
                }
            }
            .padding(.horizontal, 11)
            .padding(.vertical, 8)
            .frame(minWidth: 248, alignment: .leading)
            .background(focused ? Color.purple.opacity(0.11) : Color(nsColor: .controlBackgroundColor),
                        in: RoundedRectangle(cornerRadius: 11, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 11, style: .continuous)
                .stroke(Color.purple.opacity(focused ? 0.62 : 0.16), lineWidth: focused ? 1.5 : 1))
        }
        .buttonStyle(.plain)
        .onHover { hovering in hoveredRelationID = hovering ? relation.id : nil }
    }

    private func characterName(_ id: String) -> String {
        project.characters.first { $0.id == id }?.name ?? id
    }

    private func sceneName(_ id: String) -> String {
        project.scenes.first { $0.id == id }?.name ?? id
    }

    private func graphMetric(_ value: Int, _ label: String, color: Color) -> some View {
        VStack(spacing: 2) {
            Text("\(value)").font(.headline.monospacedDigit()).foregroundStyle(color)
            Text(label).font(.caption2).foregroundStyle(.secondary)
        }
        .frame(minWidth: 46)
    }

    private func graphBackground(_ graph: GraphLayout) -> some View {
        Canvas { context, size in
            for column in 0..<GraphNode.Kind.all.count {
                let kind = GraphNode.Kind.all[column]
                let rect = CGRect(
                    x: GraphLayout.columnCenter(column) - GraphLayout.columnPanelWidth / 2,
                    y: 14,
                    width: GraphLayout.columnPanelWidth,
                    height: size.height - 28
                )
                let path = RoundedRectangle(cornerRadius: 18, style: .continuous).path(in: rect)
                context.fill(path, with: .color(kind.color.opacity(0.026)))
                context.stroke(path, with: .color(kind.color.opacity(0.075)), lineWidth: 1)
            }
            for x in stride(from: CGFloat(14), through: size.width, by: 32) {
                for y in stride(from: CGFloat(96), through: size.height, by: 32) {
                    let dot = CGRect(x: x - 0.7, y: y - 0.7, width: 1.4, height: 1.4)
                    context.fill(Path(ellipseIn: dot), with: .color(Color.primary.opacity(0.035)))
                }
            }
        }
        .frame(width: graph.size.width, height: graph.size.height)
    }

    private func columnLabels(_ graph: GraphLayout) -> some View {
        ForEach(Array([
            appModel.localized("拍摄分段", english: "SEGMENTS"),
            appModel.localized("人物", english: "CHARACTERS"),
            appModel.localized("场景", english: "SCENES"),
            appModel.localized("道具", english: "PROPS"),
        ].enumerated()), id: \.offset) { column, title in
            HStack(spacing: 7) {
                Circle().fill(GraphNode.Kind.all[column].color).frame(width: 7, height: 7)
                Text(title).font(.caption2.bold()).foregroundStyle(.secondary).tracking(0.8)
                Spacer()
                Text("\(graph.nodes.filter { $0.kind == GraphNode.Kind.all[column] }.count)")
                    .font(.caption2.monospacedDigit()).foregroundStyle(.tertiary)
            }
            .padding(.horizontal, 11)
            .frame(width: GraphLayout.nodeWidth + 12, height: 34)
            .background(Color(nsColor: .controlBackgroundColor).opacity(0.92),
                        in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous)
                .stroke(GraphNode.Kind.all[column].color.opacity(0.12)))
            .position(x: GraphLayout.columnCenter(column), y: 36)
        }
    }

    private var graphLegend: some View {
        HStack(spacing: 15) {
            Label {
                Text(appModel.localized("分段引用", english: "Segment reference"))
            } icon: {
                Capsule().fill(Color.blue.opacity(0.45)).frame(width: 22, height: 2)
            }
            Label {
                Text(appModel.localized("人物动作", english: "Character action"))
            } icon: {
                HStack(spacing: 3) {
                    ForEach(0..<3, id: \.self) { _ in
                        Capsule().fill(Color.purple.opacity(0.72)).frame(width: 5, height: 2)
                    }
                }
            }
            Spacer()
            if hoveredNodeID != nil || highlightedRelationID != nil {
                Label(appModel.localized("已聚焦关联节点", english: "Related nodes focused"), systemImage: "scope")
                    .foregroundStyle(.teal)
            } else {
                Text(appModel.localized("将鼠标移到节点上可查看关联路径", english: "Hover a node to trace its connections"))
                    .foregroundStyle(.tertiary)
            }
        }
        .font(.caption2)
        .padding(.horizontal, 12)
        .frame(width: GraphLayout.canvasWidth - 60, height: 28)
        .position(x: GraphLayout.canvasWidth / 2, y: 80)
    }

    private func edges(_ graph: GraphLayout) -> some View {
        Canvas { context, _ in
            for edge in graph.edges {
                guard let from = graph.positions[edge.from], let to = graph.positions[edge.to] else { continue }
                let start = CGPoint(x: from.x + GraphLayout.nodeWidth / 2, y: from.y)
                let end = CGPoint(x: to.x - GraphLayout.nodeWidth / 2, y: to.y)
                let path = edgePath(edge, start: start, end: end)
                let focused = edgeIsFocused(edge)
                let hasFocus = hoveredNodeID != nil || highlightedRelationID != nil
                let opacity = hasFocus ? (focused ? 0.9 : 0.07) : (edge.relation ? 0.58 : 0.3)
                let lineColor = edge.relation ? Color.purple : edge.targetKind.color
                let width: CGFloat = focused ? (edge.relation ? 2.6 : 2.1) : (edge.relation ? 2 : 1.35)
                let style = StrokeStyle(
                    lineWidth: width,
                    lineCap: .round,
                    lineJoin: .round,
                    dash: edge.relation ? [7, 6] : []
                )
                context.stroke(
                    path,
                    with: .color(Color(nsColor: .textBackgroundColor).opacity(opacity * 0.92)),
                    style: StrokeStyle(
                        lineWidth: width + 3.2,
                        lineCap: .round,
                        lineJoin: .round,
                        dash: edge.relation ? [7, 6] : []
                    )
                )
                context.stroke(path, with: .color(lineColor.opacity(opacity)), style: style)
                context.fill(
                    Path(ellipseIn: CGRect(x: start.x - 2.5, y: start.y - 2.5, width: 5, height: 5)),
                    with: .color(lineColor.opacity(opacity))
                )
                context.fill(
                    Path(ellipseIn: CGRect(x: end.x - 2.5, y: end.y - 2.5, width: 5, height: 5)),
                    with: .color(lineColor.opacity(opacity))
                )
                if edge.relation, let relationNumber = edge.relationNumber {
                    let middle = (start.x + end.x) / 2
                    let bend = CGFloat((edge.ordinal % 5) - 2) * 8
                    let point = CGPoint(x: middle + bend, y: (start.y + end.y) / 2)
                    context.fill(
                        Path(ellipseIn: CGRect(x: point.x - 11.5, y: point.y - 11.5, width: 23, height: 23)),
                        with: .color(Color(nsColor: .textBackgroundColor).opacity(opacity))
                    )
                    context.fill(
                        Path(ellipseIn: CGRect(x: point.x - 9.5, y: point.y - 9.5, width: 19, height: 19)),
                        with: .color(Color.purple.opacity(opacity))
                    )
                    context.draw(
                        Text("\(relationNumber)").font(.caption2.bold()).foregroundStyle(Color.white.opacity(opacity)),
                        at: point
                    )
                }
            }
        }
    }

    private func edgePath(_ edge: GraphEdge, start: CGPoint, end: CGPoint) -> Path {
        var path = Path()
        path.move(to: start)
        let middle = (start.x + end.x) / 2
        let bend = CGFloat((edge.ordinal % 5) - 2) * 8
        if !edge.relation, end.x - start.x > GraphLayout.columnWidth * 1.25 {
            let laneY = CGFloat(112 + (edge.routeLane % 5) * 8)
            let exit = CGPoint(x: start.x + 23, y: laneY)
            let entry = CGPoint(x: end.x - 23, y: laneY)
            path.addCurve(
                to: exit,
                control1: .init(x: start.x + 18, y: start.y),
                control2: .init(x: exit.x, y: laneY + 20)
            )
            path.addLine(to: entry)
            path.addCurve(
                to: end,
                control1: .init(x: entry.x, y: laneY + 20),
                control2: .init(x: end.x - 18, y: end.y)
            )
        } else {
            path.addCurve(
                to: end,
                control1: .init(x: middle + bend, y: start.y),
                control2: .init(x: middle + bend, y: end.y)
            )
        }
        return path
    }

    private func edgeIsFocused(_ edge: GraphEdge) -> Bool {
        if let hoveredNodeID { return edge.from == hoveredNodeID || edge.to == hoveredNodeID }
        if let highlightedRelationID { return edge.relationID == highlightedRelationID }
        return false
    }

    private func nodeCard(_ node: GraphNode, graph: GraphLayout) -> some View {
        let focused = nodeIsFocused(node, graph: graph)
        let hasFocus = hoveredNodeID != nil || highlightedRelationID != nil
        return Button {
            switch node.kind {
            case .segment: openSegment(node.entityID)
            case .character, .scene, .prop: openAsset(node.entityID)
            }
        } label: {
            HStack(spacing: 0) {
                Capsule()
                    .fill(node.kind.color.opacity(node.isSelected || focused ? 0.9 : 0.48))
                    .frame(width: 3, height: 54)
                    .padding(.trailing, 11)
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Label(node.kind.title(appModel), systemImage: node.kind.icon)
                            .font(.caption.bold()).foregroundStyle(node.kind.color)
                        Spacer()
                        if node.complete {
                            Image(systemName: "checkmark.circle.fill").font(.caption).foregroundStyle(.green)
                        }
                    }
                    Text(node.name).font(.callout.bold()).foregroundStyle(.primary).lineLimit(1)
                    Text(node.summary).font(.caption2).foregroundStyle(.secondary).lineLimit(2)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(width: GraphLayout.nodeWidth, height: GraphLayout.nodeHeight, alignment: .leading)
            .background(Color(nsColor: .controlBackgroundColor),
                        in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .background(node.kind.color.opacity(node.isSelected || focused ? 0.1 : 0.025),
                        in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(
                    node.kind.color.opacity(node.isSelected || focused ? 0.78 : 0.18),
                    lineWidth: node.isSelected || focused ? 1.7 : 1
                ))
            .shadow(color: Color.black.opacity(node.isSelected || focused ? 0.09 : 0.035), radius: 10, y: 4)
            .opacity(hasFocus && !focused ? 0.46 : 1)
            .scaleEffect(focused ? 1.018 : 1)
            .animation(.easeOut(duration: 0.16), value: focused)
        }
        .buttonStyle(.plain)
        .onHover { hovering in hoveredNodeID = hovering ? node.id : nil }
    }

    private func nodeIsFocused(_ node: GraphNode, graph: GraphLayout) -> Bool {
        if let hoveredNodeID {
            return node.id == hoveredNodeID || graph.edges.contains {
                ($0.from == hoveredNodeID && $0.to == node.id) || ($0.to == hoveredNodeID && $0.from == node.id)
            }
        }
        if let highlightedRelationID,
           let edge = graph.edges.first(where: { $0.relationID == highlightedRelationID }) {
            return edge.from == node.id || edge.to == node.id
        }
        return node.isSelected
    }

    private var layout: GraphLayout {
        let visibleResourceIDs = Set(visibleSegments.flatMap(\.resourceIDs))
        let segmentNodes = visibleSegments.map {
            GraphNode(
                id: "segment:\($0.id)", entityID: $0.id, kind: .segment, name: $0.title,
                summary: $0.synopsis, complete: $0.video != nil, isSelected: $0.id == selectedSegmentID
            )
        }
        let characterNodes = project.characters
            .filter { showsWholeStory || selectedSegmentID == nil || visibleResourceIDs.contains($0.id) }
            .map {
                GraphNode(
                    id: "character:\($0.id)", entityID: $0.id, kind: .character, name: $0.name,
                    summary: $0.profile.roleInStory, complete: $0.media.confirmedImage != nil, isSelected: false
                )
            }
        let sceneNodes = project.scenes
            .filter { showsWholeStory || selectedSegmentID == nil || visibleResourceIDs.contains($0.id) }
            .map {
                GraphNode(
                    id: "scene:\($0.id)", entityID: $0.id, kind: .scene, name: $0.name,
                    summary: $0.profile.setting, complete: $0.media.confirmedImage != nil, isSelected: false
                )
            }
        let propNodes = project.props
            .filter { showsWholeStory || selectedSegmentID == nil || visibleResourceIDs.contains($0.id) }
            .map {
                GraphNode(
                    id: "prop:\($0.id)", entityID: $0.id, kind: .prop, name: $0.name,
                    summary: $0.description, complete: $0.media.confirmedImage != nil, isSelected: false
                )
            }
        let columns = [segmentNodes, characterNodes, sceneNodes, propNodes]
        let rows = max(columns.map(\.count).max() ?? 0, 1)
        let rowSpan = CGFloat(rows - 1) * GraphLayout.rowHeight
        var positions: [String: CGPoint] = [:]
        for (column, nodes) in columns.enumerated() {
            let columnSpan = CGFloat(max(nodes.count - 1, 0)) * GraphLayout.rowHeight
            let firstY = GraphLayout.topInset + (rowSpan - columnSpan) / 2
            for (row, node) in nodes.enumerated() {
                positions[node.id] = CGPoint(
                    x: GraphLayout.columnCenter(column),
                    y: firstY + CGFloat(row) * GraphLayout.rowHeight
                )
            }
        }

        var graphEdges: [GraphEdge] = []
        for segment in visibleSegments {
            for id in segment.characterIDs {
                graphEdges.append(.reference(
                    from: "segment:\(segment.id)", to: "character:\(id)", kind: .character,
                    ordinal: graphEdges.count
                ))
            }
            for id in segment.sceneIDs {
                graphEdges.append(.reference(
                    from: "segment:\(segment.id)", to: "scene:\(id)", kind: .scene,
                    ordinal: graphEdges.count
                ))
            }
            for id in segment.propIDs {
                graphEdges.append(.reference(
                    from: "segment:\(segment.id)", to: "prop:\(id)", kind: .prop,
                    ordinal: graphEdges.count
                ))
            }
        }
        for (index, relation) in visibleRelations.enumerated() {
            graphEdges.append(.action(
                from: "character:\(relation.characterID)", to: "scene:\(relation.sceneID)",
                relationID: relation.id, number: index + 1, ordinal: index
            ))
        }

        return GraphLayout(
            nodes: columns.flatMap { $0 },
            edges: graphEdges,
            positions: positions,
            size: CGSize(
                width: GraphLayout.canvasWidth,
                height: max(360, GraphLayout.topInset + rowSpan + 104)
            )
        )
    }
}

private struct GraphNode: Identifiable {
    enum Kind: Equatable {
        case segment
        case character
        case scene
        case prop

        static let all: [Self] = [.segment, .character, .scene, .prop]
    }

    let id: String
    let entityID: String
    let kind: Kind
    let name: String
    let summary: String
    let complete: Bool
    let isSelected: Bool
}

private extension GraphNode.Kind {
    var icon: String {
        switch self {
        case .segment: "rectangle.stack"
        case .character: "person.fill"
        case .scene: "mountain.2.fill"
        case .prop: "shippingbox.fill"
        }
    }

    var color: Color {
        switch self {
        case .segment: .blue
        case .character: .purple
        case .scene: .orange
        case .prop: .green
        }
    }

    @MainActor func title(_ model: AppModel) -> String {
        switch self {
        case .segment: model.localized("分段", english: "Segment")
        case .character: model.localized("人物", english: "Character")
        case .scene: model.localized("场景", english: "Scene")
        case .prop: model.localized("道具", english: "Prop")
        }
    }
}

private struct GraphEdge {
    let from: String
    let to: String
    let targetKind: GraphNode.Kind
    var relation = false
    var relationID: String?
    var relationNumber: Int?
    var ordinal = 0

    var routeLane: Int {
        let kindOffset: Int
        switch targetKind {
        case .segment, .character: kindOffset = 0
        case .scene: kindOffset = 1
        case .prop: kindOffset = 3
        }
        return ordinal + kindOffset
    }

    static func reference(
        from: String,
        to: String,
        kind: GraphNode.Kind,
        ordinal: Int
    ) -> Self {
        .init(from: from, to: to, targetKind: kind, ordinal: ordinal)
    }

    static func action(
        from: String,
        to: String,
        relationID: String,
        number: Int,
        ordinal: Int
    ) -> Self {
        .init(
            from: from,
            to: to,
            targetKind: .scene,
            relation: true,
            relationID: relationID,
            relationNumber: number,
            ordinal: ordinal
        )
    }
}

private struct GraphLayout {
    static let nodeWidth: CGFloat = 242
    static let nodeHeight: CGFloat = 94
    static let columnWidth: CGFloat = 310
    static let columnPanelWidth: CGFloat = 278
    static let rowHeight: CGFloat = 132
    static let topInset: CGFloat = 184
    static let canvasWidth: CGFloat = 1_240

    static func columnCenter(_ index: Int) -> CGFloat {
        155 + CGFloat(index) * columnWidth
    }

    let nodes: [GraphNode]
    let edges: [GraphEdge]
    let positions: [String: CGPoint]
    let size: CGSize
}
