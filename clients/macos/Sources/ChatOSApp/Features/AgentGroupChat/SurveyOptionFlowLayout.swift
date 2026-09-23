import SwiftUI

/// Eager adaptive layout for survey choices. `LazyVGrid` can leave already
/// measured choices undrawn when a long form is scrolled or resized on macOS.
/// Survey questions contain a bounded number of choices, so eagerly laying
/// them out is both cheap and deterministic.
struct SurveyOptionFlowLayout: Layout {
    let minItemWidth: CGFloat
    let spacing: CGFloat

    private struct Metrics {
        let columnCount: Int
        let itemWidth: CGFloat
        let rowHeights: [CGFloat]

        var height: CGFloat {
            rowHeights.reduce(0, +)
        }
    }

    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) -> CGSize {
        guard !subviews.isEmpty else { return .zero }
        let fallbackColumns = min(3, subviews.count)
        let fallbackWidth = minItemWidth * CGFloat(fallbackColumns)
            + spacing * CGFloat(max(0, fallbackColumns - 1))
        let width = max(minItemWidth, proposal.width ?? fallbackWidth)
        let metrics = metrics(width: width, subviews: subviews)
        let rowSpacing = spacing * CGFloat(max(0, metrics.rowHeights.count - 1))
        return CGSize(width: width, height: metrics.height + rowSpacing)
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) {
        guard !subviews.isEmpty else { return }
        let metrics = metrics(width: bounds.width, subviews: subviews)
        var y = bounds.minY

        for rowIndex in metrics.rowHeights.indices {
            let firstIndex = rowIndex * metrics.columnCount
            let lastIndex = min(firstIndex + metrics.columnCount, subviews.count)
            for itemIndex in firstIndex ..< lastIndex {
                let columnIndex = itemIndex - firstIndex
                let x = bounds.minX
                    + CGFloat(columnIndex) * (metrics.itemWidth + spacing)
                subviews[itemIndex].place(
                    at: CGPoint(x: x, y: y),
                    anchor: .topLeading,
                    proposal: ProposedViewSize(width: metrics.itemWidth, height: nil)
                )
            }
            y += metrics.rowHeights[rowIndex] + spacing
        }
    }

    private func metrics(width: CGFloat, subviews: Subviews) -> Metrics {
        let availableWidth = max(minItemWidth, width)
        let possibleColumns = max(
            1,
            Int((availableWidth + spacing) / (minItemWidth + spacing))
        )
        let columnCount = min(possibleColumns, max(1, subviews.count))
        let itemWidth = (
            availableWidth - spacing * CGFloat(max(0, columnCount - 1))
        ) / CGFloat(columnCount)
        let rowCount = Int(ceil(Double(subviews.count) / Double(columnCount)))
        var rowHeights = Array(repeating: CGFloat.zero, count: rowCount)

        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(
                ProposedViewSize(width: itemWidth, height: nil)
            )
            let rowIndex = index / columnCount
            rowHeights[rowIndex] = max(rowHeights[rowIndex], size.height)
        }
        return Metrics(
            columnCount: columnCount,
            itemWidth: itemWidth,
            rowHeights: rowHeights
        )
    }
}
