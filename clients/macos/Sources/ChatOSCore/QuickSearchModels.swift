import Foundation

public enum QuickSearchResultKind: String, Sendable, Hashable, CaseIterable {
    case suggestion
    case chatOS
    case application
    case file
    case action
}

public enum QuickSearchBuiltInAction: String, Sendable, Hashable {
    case screenshot
    case screenRecording
    case clipboardHistory
    case openSettings
    case openRuntimePermissions
}

public enum QuickSearchAction: Sendable, Hashable {
    case openProject(String)
    case openContact(String)
    case openApplication(URL)
    case openFile(URL)
    case revealFile(URL)
    case builtIn(QuickSearchBuiltInAction)
}

public struct QuickSearchResult: Identifiable, Sendable, Hashable {
    public let id: String
    public let kind: QuickSearchResultKind
    public let title: String
    public let subtitle: String?
    public let systemImage: String
    public let score: Double
    public let action: QuickSearchAction

    public init(
        id: String,
        kind: QuickSearchResultKind,
        title: String,
        subtitle: String?,
        systemImage: String,
        score: Double,
        action: QuickSearchAction
    ) {
        self.id = id
        self.kind = kind
        self.title = title
        self.subtitle = subtitle
        self.systemImage = systemImage
        self.score = score
        self.action = action
    }
}

public enum QuickSearchRanking {
    public static func score(
        query: String,
        title: String,
        subtitle: String? = nil,
        providerWeight: Double = 0,
        recencyBoost: Double = 0,
        frequencyBoost: Double = 0
    ) -> Double? {
        let normalizedQuery = normalize(query)
        guard !normalizedQuery.isEmpty else {
            return providerWeight + recencyBoost + frequencyBoost
        }

        let normalizedTitle = normalize(title)
        let normalizedSubtitle = normalize(subtitle ?? "")
        let titleScore = textualScore(query: normalizedQuery, candidate: normalizedTitle)
        let subtitleScore = textualScore(query: normalizedQuery, candidate: normalizedSubtitle)
            .map { $0 * 0.45 }
        guard let textScore = [titleScore, subtitleScore].compactMap({ $0 }).max() else {
            return nil
        }
        return providerWeight + textScore + min(80, recencyBoost) + min(45, frequencyBoost)
    }

    public static func sorted(_ results: [QuickSearchResult]) -> [QuickSearchResult] {
        results.sorted {
            if $0.score == $1.score {
                return $0.title.localizedStandardCompare($1.title) == .orderedAscending
            }
            return $0.score > $1.score
        }
    }

    private static func textualScore(query: String, candidate: String) -> Double? {
        guard !candidate.isEmpty else { return nil }
        if candidate == query { return 520 }
        if candidate.hasPrefix(query) { return 390 - Double(candidate.count - query.count) * 0.15 }
        if candidate.range(of: " \(query)") != nil
            || candidate.range(of: "-\(query)") != nil
            || candidate.range(of: "_\(query)") != nil {
            return 310
        }
        if let range = candidate.range(of: query) {
            return 235 - Double(candidate.distance(from: candidate.startIndex, to: range.lowerBound)) * 0.5
        }

        var candidateIndex = candidate.startIndex
        var gapPenalty = 0.0
        var previousMatch: String.Index?
        for character in query {
            guard let match = candidate[candidateIndex...].firstIndex(of: character) else { return nil }
            if let previousMatch {
                gapPenalty += Double(candidate.distance(from: previousMatch, to: match) - 1) * 3
            } else {
                gapPenalty += Double(candidate.distance(from: candidate.startIndex, to: match)) * 2
            }
            previousMatch = match
            candidateIndex = candidate.index(after: match)
        }
        return max(40, 170 - gapPenalty)
    }

    private static func normalize(_ value: String) -> String {
        value.folding(options: [.caseInsensitive, .diacriticInsensitive, .widthInsensitive], locale: .current)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
