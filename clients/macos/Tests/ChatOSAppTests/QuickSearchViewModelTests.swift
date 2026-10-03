@testable import ChatOSApp
import Testing

struct QuickSearchViewModelTests {
    @Test("usage history keeps only the most recent bounded records")
    func usageHistoryIsBoundedByRecency() {
        let values = Dictionary(uniqueKeysWithValues: (0..<700).map { index in
            (
                "result-\(index)",
                QuickSearchUsageRecord(lastUsedAt: Double(index), count: index + 1)
            )
        })

        let pruned = QuickSearchViewModel.prunedUsage(values)

        #expect(pruned.count == QuickSearchViewModel.maximumUsageRecordCount)
        #expect(pruned["result-699"] != nil)
        #expect(pruned["result-188"] != nil)
        #expect(pruned["result-187"] == nil)
        #expect(pruned["result-0"] == nil)
    }

    @Test("usage history is unchanged while it remains below the cap")
    func usageHistoryBelowCapIsUnchanged() {
        let values = [
            "a": QuickSearchUsageRecord(lastUsedAt: 2, count: 1),
            "b": QuickSearchUsageRecord(lastUsedAt: 1, count: 3),
        ]

        #expect(QuickSearchViewModel.prunedUsage(values) == values)
    }
}
