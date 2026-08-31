import Foundation
import SwiftUI

struct MarkdownDocumentView: View {
    private let blocks: [MarkdownBlock]
    private let allowsTextSelection: Bool

    init(markdown: String, allowsTextSelection: Bool = true) {
        blocks = MarkdownBlockParser.parse(markdown)
        self.allowsTextSelection = allowsTextSelection
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 13) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                MarkdownBlockView(
                    block: block,
                    allowsTextSelection: allowsTextSelection
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct MarkdownBlockView: View {
    let block: MarkdownBlock
    let allowsTextSelection: Bool

    @ViewBuilder
    var body: some View {
        switch block {
        case let .heading(level, text):
            MarkdownInlineText(text: text, allowsTextSelection: allowsTextSelection)
                .appFont(headingFont(level))
                .padding(.top, level <= 2 ? 4 : 0)

        case let .paragraph(text):
            MarkdownInlineText(text: text, allowsTextSelection: allowsTextSelection)
                .appFont(.callout)
                .lineSpacing(4)

        case let .list(items):
            VStack(alignment: .leading, spacing: 7) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text(item.marker)
                            .appFont(.callout.monospacedDigit().weight(.semibold))
                            .foregroundStyle(AppPalette.ai)
                            .frame(minWidth: 15, alignment: .trailing)
                        MarkdownInlineText(
                            text: item.text,
                            allowsTextSelection: allowsTextSelection
                        )
                            .appFont(.callout)
                            .lineSpacing(3)
                    }
                    .padding(.leading, CGFloat(item.depth) * 18)
                }
            }

        case let .quote(text):
            HStack(alignment: .top, spacing: 10) {
                RoundedRectangle(cornerRadius: 1)
                    .fill(AppPalette.ai.opacity(0.55))
                    .frame(width: 3)
                MarkdownInlineText(text: text, allowsTextSelection: allowsTextSelection)
                    .appFont(.callout)
                    .foregroundStyle(.secondary)
                    .lineSpacing(3)
            }
            .padding(.vertical, 3)

        case let .code(language, content):
            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Text(language?.isEmpty == false ? language! : "代码")
                        .appFont(.caption2.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .padding(.horizontal, 11)
                .padding(.vertical, 7)
                .background(AppPalette.surfaceSubtle)

                ScrollView(.horizontal) {
                    Text(content)
                        .appFont(.caption.monospaced())
                        .lineSpacing(3)
                        .appTextSelection(allowsTextSelection)
                        .fixedSize(horizontal: true, vertical: false)
                        .padding(11)
                }
                .background(Color(nsColor: .textBackgroundColor))
            }
            .clipShape(RoundedRectangle(cornerRadius: 9))
            .overlay {
                RoundedRectangle(cornerRadius: 9)
                    .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
            }

        case .divider:
            Divider().padding(.vertical, 2)

        case let .table(headers, rows):
            MarkdownTableView(
                headers: headers,
                rows: rows,
                allowsTextSelection: allowsTextSelection
            )
        }
    }

    private func headingFont(_ level: Int) -> AppFontSpec {
        switch level {
        case 1: .title3.weight(.bold)
        case 2: .headline.weight(.bold)
        case 3: .subheadline.weight(.bold)
        default: .callout.weight(.semibold)
        }
    }
}

private struct MarkdownInlineText: View {
    let text: String
    let allowsTextSelection: Bool

    var body: some View {
        Text(rendered)
            .appTextSelection(allowsTextSelection)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var rendered: AttributedString {
        (try? AttributedString(
            markdown: text,
            options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        )) ?? AttributedString(text)
    }
}

private struct MarkdownTableView: View {
    let headers: [String]
    let rows: [[String]]
    let allowsTextSelection: Bool

    var body: some View {
        ScrollView(.horizontal) {
            Grid(alignment: .leading, horizontalSpacing: 0, verticalSpacing: 0) {
                GridRow {
                    ForEach(Array(headers.enumerated()), id: \.offset) { _, header in
                        tableCell(header, isHeader: true)
                    }
                }
                ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                    GridRow {
                        ForEach(0..<headers.count, id: \.self) { column in
                            tableCell(column < row.count ? row[column] : "", isHeader: false)
                        }
                    }
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay {
                RoundedRectangle(cornerRadius: 8)
                    .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
            }
        }
    }

    private func tableCell(_ text: String, isHeader: Bool) -> some View {
        MarkdownInlineText(text: text, allowsTextSelection: allowsTextSelection)
            .appFont(.caption.weight(isHeader ? .semibold : .regular))
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .frame(minWidth: 110, alignment: .leading)
            .background(isHeader ? AppPalette.surfaceSubtle : .clear)
            .overlay(alignment: .trailing) {
                Rectangle()
                    .fill(Color(nsColor: .separatorColor))
                    .frame(width: 1)
            }
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(Color(nsColor: .separatorColor))
                    .frame(height: 1)
            }
    }
}

extension View {
    @ViewBuilder
    func appTextSelection(_ isEnabled: Bool) -> some View {
        if isEnabled {
            textSelection(.enabled)
        } else {
            self
        }
    }
}

private enum MarkdownBlock: Equatable {
    case heading(level: Int, text: String)
    case paragraph(String)
    case list([MarkdownListItem])
    case quote(String)
    case code(language: String?, content: String)
    case divider
    case table(headers: [String], rows: [[String]])
}

private struct MarkdownListItem: Equatable {
    var marker: String
    var text: String
    var depth: Int
}

private enum MarkdownBlockParser {
    static func parse(_ source: String) -> [MarkdownBlock] {
        let normalized = source
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        let lines = expandedMarkdownLines(normalized)
        var blocks: [MarkdownBlock] = []
        var paragraph: [String] = []
        var listItems: [MarkdownListItem] = []
        var index = 0

        func flushParagraph() {
            let value = paragraph
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty }
                .joined(separator: " ")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if !value.isEmpty { blocks.append(.paragraph(value)) }
            paragraph.removeAll(keepingCapacity: true)
        }

        func flushList() {
            if !listItems.isEmpty { blocks.append(.list(listItems)) }
            listItems.removeAll(keepingCapacity: true)
        }

        func flushTextBlocks() {
            flushParagraph()
            flushList()
        }

        while index < lines.count {
            let line = lines[index]
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            if trimmed.isEmpty {
                flushTextBlocks()
                index += 1
                continue
            }

            if isCodeFence(line) {
                flushTextBlocks()
                let language = codeFenceLanguage(line)
                var codeLines: [String] = []
                index += 1
                while index < lines.count, !isCodeFence(lines[index]) {
                    codeLines.append(lines[index])
                    index += 1
                }
                if index < lines.count { index += 1 }
                blocks.append(.code(language: language, content: codeLines.joined(separator: "\n")))
                continue
            }

            if let heading = heading(line) {
                flushTextBlocks()
                blocks.append(.heading(level: heading.level, text: heading.text))
                index += 1
                continue
            }

            if isDivider(trimmed) {
                flushTextBlocks()
                blocks.append(.divider)
                index += 1
                continue
            }

            if index + 1 < lines.count,
               let headers = tableRow(line),
               isTableSeparator(lines[index + 1], expectedColumns: headers.count) {
                flushTextBlocks()
                var rows: [[String]] = []
                index += 2
                while index < lines.count,
                      !lines[index].trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                      let row = tableRow(lines[index]) {
                    rows.append(row)
                    index += 1
                }
                blocks.append(.table(headers: headers, rows: rows))
                continue
            }

            if trimmed.hasPrefix(">") {
                flushTextBlocks()
                var quoteLines: [String] = []
                while index < lines.count {
                    let quoteLine = lines[index].trimmingCharacters(in: .whitespaces)
                    guard quoteLine.hasPrefix(">") else { break }
                    quoteLines.append(
                        String(quoteLine.dropFirst()).trimmingCharacters(in: .whitespaces)
                    )
                    index += 1
                }
                blocks.append(.quote(quoteLines.joined(separator: "\n")))
                continue
            }

            if let item = listItem(line) {
                flushParagraph()
                listItems.append(item)
                index += 1
                continue
            }

            if !listItems.isEmpty, leadingSpaceCount(line) > 0 {
                listItems[listItems.count - 1].text += " " + trimmed
                index += 1
                continue
            }

            flushList()
            paragraph.append(line)
            index += 1
        }

        flushTextBlocks()
        return blocks.isEmpty ? [.paragraph(source)] : blocks
    }

    private static func isCodeFence(_ line: String) -> Bool {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        return trimmed.hasPrefix("```")
    }

    private static func codeFenceLanguage(_ line: String) -> String? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        return String(trimmed.dropFirst(3)).trimmingCharacters(in: .whitespaces).nilIfEmpty
    }

    private static func heading(_ line: String) -> (level: Int, text: String)? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        let level = trimmed.prefix(while: { $0 == "#" }).count
        guard (1...6).contains(level) else { return nil }
        let remainder = trimmed.dropFirst(level)
        guard remainder.first?.isWhitespace == true else { return nil }
        let text = String(remainder).trimmingCharacters(in: .whitespaces)
        return text.isEmpty ? nil : (level, text)
    }

    private static func listItem(_ line: String) -> MarkdownListItem? {
        let spaces = leadingSpaceCount(line)
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        for prefix in ["- ", "* ", "+ "] where trimmed.hasPrefix(prefix) {
            return MarkdownListItem(
                marker: "•",
                text: String(trimmed.dropFirst(prefix.count)),
                depth: spaces / 2
            )
        }

        let digits = trimmed.prefix(while: { $0.isNumber })
        guard !digits.isEmpty else { return nil }
        let remainder = trimmed.dropFirst(digits.count)
        guard remainder.hasPrefix(". ") || remainder.hasPrefix(") ") else { return nil }
        return MarkdownListItem(
            marker: "\(digits).",
            text: String(remainder.dropFirst(2)),
            depth: spaces / 2
        )
    }

    private static func tableRow(_ line: String) -> [String]? {
        guard line.contains("|") else { return nil }
        var value = line.trimmingCharacters(in: .whitespaces)
        if value.hasPrefix("|") { value.removeFirst() }
        if value.hasSuffix("|") { value.removeLast() }
        let cells = value.split(separator: "|", omittingEmptySubsequences: false)
            .map { String($0).trimmingCharacters(in: .whitespaces) }
        return cells.count >= 2 ? cells : nil
    }

    private static func isTableSeparator(_ line: String, expectedColumns: Int) -> Bool {
        guard let cells = tableRow(line), cells.count == expectedColumns else { return false }
        return cells.allSatisfy { cell in
            let core = cell.trimmingCharacters(in: CharacterSet(charactersIn: ":"))
            return core.count >= 3 && core.allSatisfy { $0 == "-" }
        }
    }

    private static func isDivider(_ line: String) -> Bool {
        let compact = line.filter { !$0.isWhitespace }
        guard compact.count >= 3, let first = compact.first,
              [Character("-"), Character("*"), Character("_")].contains(first) else { return false }
        return compact.allSatisfy { $0 == first }
    }

    private static func leadingSpaceCount(_ line: String) -> Int {
        line.prefix(while: { $0 == " " || $0 == "\t" }).reduce(0) { count, character in
            count + (character == "\t" ? 2 : 1)
        }
    }

    private static func expandedMarkdownLines(_ source: String) -> [String] {
        let rawLines = source.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        var result: [String] = []
        var isInsideCodeFence = false

        for line in rawLines {
            if isCodeFence(line) {
                isInsideCodeFence.toggle()
                result.append(line)
            } else if isInsideCodeFence {
                result.append(line)
            } else {
                result.append(contentsOf: splitInlineOrderedList(line))
            }
        }
        return result
    }

    private static func splitInlineOrderedList(_ line: String) -> [String] {
        let pattern = #"(?<![0-9])([0-9]{1,2})[\)）][ \t]*"#
        guard let expression = try? NSRegularExpression(pattern: pattern) else { return [line] }
        let fullRange = NSRange(line.startIndex..<line.endIndex, in: line)
        let matches = expression.matches(in: line, range: fullRange)
        guard matches.count >= 2 else { return [line] }

        let numbers = matches.compactMap { match -> Int? in
            guard let range = Range(match.range(at: 1), in: line) else { return nil }
            return Int(line[range])
        }
        guard numbers.count == matches.count,
              numbers.first == 1,
              numbers.enumerated().allSatisfy({ $0.element == $0.offset + 1 }) else {
            return [line]
        }

        var parts: [String] = []
        if let firstRange = Range(matches[0].range, in: line) {
            let prefix = line[..<firstRange.lowerBound].trimmingCharacters(in: .whitespaces)
            if !prefix.isEmpty { parts.append(prefix) }
        }

        for (offset, match) in matches.enumerated() {
            guard let markerRange = Range(match.range, in: line) else { continue }
            let contentEnd: String.Index
            if offset + 1 < matches.count,
               let nextRange = Range(matches[offset + 1].range, in: line) {
                contentEnd = nextRange.lowerBound
            } else {
                contentEnd = line.endIndex
            }
            let content = line[markerRange.upperBound..<contentEnd]
                .trimmingCharacters(in: .whitespaces)
            parts.append("\(numbers[offset]). \(content)")
        }
        return parts
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
