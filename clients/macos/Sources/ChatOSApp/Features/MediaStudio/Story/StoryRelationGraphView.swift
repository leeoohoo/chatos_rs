import ChatOSCore
import SwiftUI

struct StoryRelationGraphView: View {
    @EnvironmentObject private var appModel: AppModel
    let project: StoryProject
    let selectedSegmentID: String?
    let openAsset: (String) -> Void
    let openSegment: (String) -> Void
    @State private var showsWholeStory = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 5) {
                    Text(appModel.localized("人物、场景与分段关系图", english: "Character, Scene and Segment Graph")).font(.title2.bold())
                    Text(appModel.localized("点击人物、场景或道具可制作素材图；点击分段可打开对应的拍摄提示词。", english: "Select a character, scene or prop to produce its image, or select a segment to open its shot prompts."))
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                Toggle(appModel.localized("显示全剧", english: "Show Full Story"), isOn: $showsWholeStory)
                    .toggleStyle(.switch).controlSize(.small).disabled(selectedSegmentID == nil)
                legend
            }
            if project.segments.isEmpty && project.resources.isEmpty {
                ContentUnavailableView(appModel.localized("尚无关系数据", english: "No Relationship Data"),
                                       systemImage: "point.3.connected.trianglepath.dotted",
                                       description: Text(appModel.localized("完成全剧规划后，这里会显示分段引用和人物在场景中的关系。", english: "After story planning, segment references and character-in-scene relationships appear here.")))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView([.horizontal, .vertical]) {
                    let graph = layout
                    ZStack(alignment: .topLeading) {
                        Color(nsColor: .textBackgroundColor)
                        edges(graph)
                        ForEach(graph.nodes) { node in
                            nodeCard(node).position(graph.positions[node.id] ?? .zero)
                        }
                    }.frame(width: graph.size.width, height: graph.size.height)
                }
                .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
                .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.primary.opacity(0.08)))
            }
        }.padding(16).background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    private var legend: some View {
        HStack(spacing: 12) {
            Label(appModel.localized("分段引用", english: "Segment Reference"), systemImage: "line.diagonal")
                .foregroundStyle(.blue)
            Label(appModel.localized("人物在场景中", english: "Character in Scene"), systemImage: "line.diagonal")
                .foregroundStyle(.purple)
        }.font(.caption)
    }

    private func edges(_ graph: GraphLayout) -> some View {
        Canvas { context, _ in
            for edge in graph.edges {
                guard let from = graph.positions[edge.from], let to = graph.positions[edge.to] else { continue }
                let start = CGPoint(x: from.x + GraphLayout.nodeWidth / 2, y: from.y)
                let end = CGPoint(x: to.x - GraphLayout.nodeWidth / 2, y: to.y)
                var path = Path(); path.move(to: start)
                let middle = (start.x + end.x) / 2
                path.addCurve(to: end, control1: .init(x: middle, y: start.y), control2: .init(x: middle, y: end.y))
                context.stroke(path, with: .color(edge.relation ? .purple.opacity(0.72) : .blue.opacity(0.48)),
                               style: StrokeStyle(lineWidth: edge.relation ? 2 : 1.4,
                                                  dash: edge.relation ? [7, 5] : []))
                if edge.relation, let label = edge.label, !label.isEmpty {
                    let point = CGPoint(x: middle, y: (start.y + end.y) / 2 - 10)
                    context.draw(Text(label).font(.caption2).foregroundStyle(.purple), at: point)
                }
            }
        }
    }

    private func nodeCard(_ node: GraphNode) -> some View {
        Button {
            switch node.kind {
            case .segment: openSegment(node.entityID)
            case .character, .scene, .prop: openAsset(node.entityID)
            }
        } label: {
            VStack(alignment: .leading, spacing: 5) {
                HStack {
                    Label(node.kind.title(appModel), systemImage: node.kind.icon).font(.caption.bold())
                        .foregroundStyle(node.kind.color)
                    Spacer()
                    if node.complete { Image(systemName: "checkmark.circle.fill").foregroundStyle(.green) }
                }
                Text(node.name).font(.callout.bold()).foregroundStyle(.primary).lineLimit(1)
                Text(node.summary).font(.caption2).foregroundStyle(.secondary).lineLimit(2)
            }
            .padding(11).frame(width: GraphLayout.nodeWidth, height: GraphLayout.nodeHeight, alignment: .leading)
            .background(node.kind.color.opacity(node.isSelected ? 0.16 : 0.07), in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(node.kind.color.opacity(node.isSelected ? 0.9 : 0.32), lineWidth: node.isSelected ? 2 : 1))
        }.buttonStyle(.plain)
    }

    private var layout: GraphLayout {
        let visibleSegments = showsWholeStory || selectedSegmentID == nil
            ? project.segments
            : project.segments.filter { $0.id == selectedSegmentID }
        let visibleResourceIDs = Set(visibleSegments.flatMap(\.resourceIDs))
        let segmentNodes = visibleSegments.map {
            GraphNode(id: "segment:\($0.id)", entityID: $0.id, kind: .segment, name: $0.title,
                      summary: $0.synopsis, complete: $0.video != nil, isSelected: $0.id == selectedSegmentID)
        }
        let characterNodes = project.characters.filter { showsWholeStory || selectedSegmentID == nil || visibleResourceIDs.contains($0.id) }.map {
            GraphNode(id: "character:\($0.id)", entityID: $0.id, kind: .character, name: $0.name,
                      summary: $0.profile.roleInStory, complete: $0.media.confirmedImage != nil, isSelected: false)
        }
        let sceneNodes = project.scenes.filter { showsWholeStory || selectedSegmentID == nil || visibleResourceIDs.contains($0.id) }.map {
            GraphNode(id: "scene:\($0.id)", entityID: $0.id, kind: .scene, name: $0.name,
                      summary: $0.profile.setting, complete: $0.media.confirmedImage != nil, isSelected: false)
        }
        let propNodes = project.props.filter { showsWholeStory || selectedSegmentID == nil || visibleResourceIDs.contains($0.id) }.map {
            GraphNode(id: "prop:\($0.id)", entityID: $0.id, kind: .prop, name: $0.name,
                      summary: $0.description, complete: $0.media.confirmedImage != nil, isSelected: false)
        }
        let columns = [segmentNodes, characterNodes, sceneNodes, propNodes]
        var positions: [String: CGPoint] = [:]
        for (column, nodes) in columns.enumerated() {
            for (row, node) in nodes.enumerated() {
                positions[node.id] = CGPoint(x: 145 + CGFloat(column) * 300, y: 70 + CGFloat(row) * 112)
            }
        }
        var edges: [GraphEdge] = []
        for segment in visibleSegments {
            for id in segment.characterIDs { edges.append(.init(from: "segment:\(segment.id)", to: "character:\(id)")) }
            for id in segment.sceneIDs { edges.append(.init(from: "segment:\(segment.id)", to: "scene:\(id)")) }
            for id in segment.propIDs { edges.append(.init(from: "segment:\(segment.id)", to: "prop:\(id)")) }
        }
        for relation in project.relations where visibleSegments.contains(where: { alter in alter.id == relation.segmentID }) {
            edges.append(.init(from: "character:\(relation.characterID)", to: "scene:\(relation.sceneID)", relation: true,
                               label: "\(relation.action) · \(relation.startSecond)–\(relation.endSecond)s"))
        }
        let rows = max(columns.map(\.count).max() ?? 0, 5)
        return .init(nodes: columns.flatMap { $0 }, edges: edges, positions: positions,
                     size: CGSize(width: 1_150, height: CGFloat(rows) * 112 + 50))
    }
}

private struct GraphNode: Identifiable {
    enum Kind { case segment, character, scene, prop }
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
        switch self { case .segment: "rectangle.stack"; case .character: "person.fill"; case .scene: "mountain.2.fill"; case .prop: "shippingbox.fill" }
    }
    var color: Color {
        switch self { case .segment: .blue; case .character: .purple; case .scene: .orange; case .prop: .green }
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
    var relation = false
    var label: String?
}

private struct GraphLayout {
    static let nodeWidth: CGFloat = 240
    static let nodeHeight: CGFloat = 84
    let nodes: [GraphNode]
    let edges: [GraphEdge]
    let positions: [String: CGPoint]
    let size: CGSize
}
