import ChatOSCore
import Foundation

struct ComposerCapabilityCandidate: Identifiable, Equatable {
    let selection: ConversationExplicitCapabilitySelection
    let subtitle: String

    var id: String {
        switch selection.kind {
        case .plugin:
            ["plugin", selection.pluginKey ?? "", selection.componentKey ?? ""].joined(separator: ":")
        case .requirementSurvey:
            "requirement_survey"
        }
    }
}

struct ComposerCapabilityQuery: Equatable {
    let range: Range<String.Index>
    let value: String
}

enum ComposerCapabilitySyntax {
    static func trailingQuery(in text: String) -> ComposerCapabilityQuery? {
        guard let slashIndex = text.lastIndex(of: "/") else { return nil }
        if slashIndex != text.startIndex {
            let previous = text[text.index(before: slashIndex)]
            guard previous.isWhitespace || previous.isNewline else { return nil }
        }
        let queryStart = text.index(after: slashIndex)
        let query = text[queryStart...]
        guard query.count <= 80,
              !query.contains(where: { $0.isWhitespace || $0.isNewline }) else {
            return nil
        }
        return .init(range: slashIndex..<text.endIndex, value: String(query))
    }

    static func removingTrailingQuery(_ query: ComposerCapabilityQuery, from text: String) -> String {
        var result = text
        result.removeSubrange(query.range)
        return result
    }
}

enum ComposerCapabilityMenuLayout {
    static let rowHeight: CGFloat = 55
    static let rowSpacing: CGFloat = 2
    static let maximumHeight: CGFloat = 360

    static func height(candidateCount: Int) -> CGFloat {
        guard candidateCount > 0 else { return 0 }
        let rowsHeight = CGFloat(candidateCount) * rowHeight
        let spacingHeight = CGFloat(candidateCount - 1) * rowSpacing
        return min(maximumHeight, rowsHeight + spacingHeight)
    }
}
