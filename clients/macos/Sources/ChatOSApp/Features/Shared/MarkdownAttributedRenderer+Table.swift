import AppKit
import Foundation

@MainActor
extension MarkdownAttributedRenderer {
    static func appendTable(
        headers: [String],
        rows: [[String]],
        to result: NSMutableAttributedString
    ) {
        let columnCount = max(1, headers.count)
        let table = NSTextTable()
        table.numberOfColumns = columnCount
        table.layoutAlgorithm = .fixedLayoutAlgorithm
        table.collapsesBorders = true
        table.hidesEmptyCells = false
        table.setContentWidth(100, type: .percentageValueType)

        let values = [headers] + rows
        for (rowIndex, rawRow) in values.enumerated() {
            let row = normalizedTableRow(rawRow, columnCount: columnCount)
            for columnIndex in 0..<columnCount {
                let block = NSTextTableBlock(
                    table: table,
                    startingRow: rowIndex,
                    rowSpan: 1,
                    startingColumn: columnIndex,
                    columnSpan: 1
                )
                block.verticalAlignment = .topAlignment
                block.setWidth(6, type: .absoluteValueType, for: .padding)
                block.setWidth(0.5, type: .absoluteValueType, for: .border)
                block.setBorderColor(subtleBorderColor)
                if rowIndex == 0 {
                    block.backgroundColor = tableHeaderBackgroundColor
                } else if rowIndex.isMultiple(of: 2) {
                    block.backgroundColor = tableAlternateBackgroundColor
                }

                let paragraph = NSMutableParagraphStyle()
                paragraph.textBlocks = [block]
                paragraph.lineSpacing = 2
                paragraph.paragraphSpacing = 0
                let font = NSFont.systemFont(
                    ofSize: 12,
                    weight: rowIndex == 0 ? .semibold : .regular
                )
                let cell = NSMutableAttributedString(
                    attributedString: inline(row[columnIndex], font: font)
                )
                if cell.length == 0 {
                    cell.append(NSAttributedString(string: " "))
                }
                cell.append(NSAttributedString(string: "\n"))
                cell.addAttribute(
                    .paragraphStyle,
                    value: paragraph,
                    range: NSRange(location: 0, length: cell.length)
                )
                result.append(cell)
            }
        }
    }

    private static func normalizedTableRow(
        _ row: [String],
        columnCount: Int
    ) -> [String] {
        if row.count == columnCount { return row }
        if row.count < columnCount {
            return row + Array(repeating: "", count: columnCount - row.count)
        }
        return Array(row.prefix(columnCount - 1))
            + [row.dropFirst(columnCount - 1).joined(separator: " | ")]
    }
}
