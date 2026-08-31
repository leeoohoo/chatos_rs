import ChatOSCore
import Testing

struct QuickSearchRankingTests {
    @Test
    func exactAndPrefixMatchesOutrankFuzzyMatches() throws {
        let exact = try #require(QuickSearchRanking.score(query: "Safari", title: "Safari"))
        let prefix = try #require(QuickSearchRanking.score(query: "Saf", title: "Safari"))
        let fuzzy = try #require(QuickSearchRanking.score(query: "sfr", title: "Safari"))

        #expect(exact > prefix)
        #expect(prefix > fuzzy)
    }

    @Test
    func chineseSubstringMatchesWithoutTokenization() throws {
        let score = try #require(QuickSearchRanking.score(
            query: "项目",
            title: "打开项目设置"
        ))
        #expect(score > 200)
    }

    @Test
    func unrelatedTextDoesNotMatch() {
        #expect(QuickSearchRanking.score(query: "terminal", title: "Safari") == nil)
    }

    @Test
    func recencyCannotOvertakeAHighlyAccurateMatch() throws {
        let exact = try #require(QuickSearchRanking.score(query: "Notes", title: "Notes"))
        let fuzzyRecent = try #require(QuickSearchRanking.score(
            query: "nts",
            title: "Network Tools",
            recencyBoost: 1_000,
            frequencyBoost: 1_000
        ))
        #expect(exact > fuzzyRecent)
    }
}
