import Foundation

enum MarkdownBlock: Equatable, Sendable {
    case heading(level: Int, text: String)
    case paragraph(String)
    case list([MarkdownListItem])
    case quote(String)
    case code(language: String?, content: String)
    case divider
    case table(headers: [String], rows: [[String]])
}

struct MarkdownListItem: Equatable, Sendable {
    var marker: String
    var text: String
    var depth: Int
}

enum MarkdownBlockParser {
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
