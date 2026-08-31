import ChatOSCore
import CoreGraphics

struct MessageTaskGraphLayout {
    static let nodeSize = CGSize(width: 232, height: 142)
    var positions: [String: CGPoint]
    var contentSize: CGSize

    static func make(graph: MessageTaskGraphSnapshot) -> Self {
        let horizontalGap: CGFloat = 84
        let verticalGap: CGFloat = 46
        let margin: CGFloat = 54
        let columns = Dictionary(grouping: graph.nodes) { max(0, $0.depth) }
        let orderedDepths = columns.keys.sorted()
        let columnCount = max(1, orderedDepths.count)
        let maxRows = max(1, columns.values.map(\.count).max() ?? 1)
        let contentHeight = max(
            520,
            margin * 2 + CGFloat(maxRows) * nodeSize.height
                + CGFloat(maxRows - 1) * verticalGap
        )
        let contentWidth = max(
            800,
            margin * 2 + CGFloat(columnCount) * nodeSize.width
                + CGFloat(max(0, columnCount - 1)) * horizontalGap
        )
        var positions: [String: CGPoint] = [:]
        for (columnIndex, depth) in orderedDepths.enumerated() {
            let nodes = (columns[depth] ?? []).sorted {
                if $0.isCurrentMessage != $1.isCurrentMessage { return $0.isCurrentMessage }
                return $0.task.title.localizedStandardCompare($1.task.title) == .orderedAscending
            }
            let columnHeight = CGFloat(nodes.count) * nodeSize.height
                + CGFloat(max(0, nodes.count - 1)) * verticalGap
            let startY = max(margin, (contentHeight - columnHeight) / 2)
            for (index, node) in nodes.enumerated() {
                positions[node.id] = CGPoint(
                    x: margin + nodeSize.width / 2
                        + CGFloat(columnIndex) * (nodeSize.width + horizontalGap),
                    y: startY + nodeSize.height / 2
                        + CGFloat(index) * (nodeSize.height + verticalGap)
                )
            }
        }
        return Self(positions: positions, contentSize: CGSize(width: contentWidth, height: contentHeight))
    }
}
