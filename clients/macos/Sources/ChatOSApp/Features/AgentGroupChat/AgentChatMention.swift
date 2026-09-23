import Foundation

struct AgentChatMentionCandidate: Identifiable, Equatable {
    let id: String
    let name: String
    let subtitle: String?
}

struct AgentChatMentionQuery: Equatable {
    let range: Range<String.Index>
    let value: String
}

enum AgentChatMentionSyntax {
    static func trailingQuery(in text: String) -> AgentChatMentionQuery? {
        guard let atIndex = text.lastIndex(of: "@") else { return nil }
        if atIndex != text.startIndex {
            let previous = text[text.index(before: atIndex)]
            guard isBoundary(previous) else { return nil }
        }

        let queryStart = text.index(after: atIndex)
        let query = text[queryStart...]
        guard query.count <= 80,
              !query.contains(where: { $0.isWhitespace || $0.isNewline }) else {
            return nil
        }
        return AgentChatMentionQuery(
            range: atIndex..<text.endIndex,
            value: String(query)
        )
    }

    static func containsMention(named name: String, in text: String) -> Bool {
        let normalizedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedName.isEmpty else { return false }
        let needle = "@\(normalizedName)"
        var searchStart = text.startIndex
        while searchStart < text.endIndex,
              let range = text.range(
                of: needle,
                options: [.caseInsensitive, .diacriticInsensitive],
                range: searchStart..<text.endIndex
              ) {
            if range.upperBound == text.endIndex || isBoundary(text[range.upperBound]) {
                return true
            }
            searchStart = range.upperBound
        }
        return false
    }

    static func removingTrailingQuery(
        _ query: AgentChatMentionQuery,
        from text: String
    ) -> String {
        var result = text
        result.removeSubrange(query.range)
        return result
    }

    private static func isBoundary(_ character: Character) -> Bool {
        character.isWhitespace
            || character.isNewline
            || character.unicodeScalars.allSatisfy {
                CharacterSet.punctuationCharacters.contains($0)
                    || CharacterSet.symbols.contains($0)
            }
    }
}
