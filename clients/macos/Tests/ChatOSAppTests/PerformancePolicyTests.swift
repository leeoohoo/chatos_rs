import Foundation
import Testing
@testable import ChatOSApp

@Suite("Performance policies")
struct PerformancePolicyTests {
    @Test("conversation recency is bounded and refreshes recently used entries")
    func conversationRecencyIsBounded() {
        var recency = ConversationCacheRecency(capacity: 3)
        #expect(recency.touch("a").isEmpty)
        #expect(recency.touch("b").isEmpty)
        #expect(recency.touch("c").isEmpty)
        #expect(recency.touch("a").isEmpty)
        #expect(recency.touch("d") == ["b"])
        #expect(recency.sessionIDs == ["c", "a", "d"])
    }

    @Test("conversation recency does not evict protected sessions")
    func conversationRecencyProtectsActiveSession() {
        var recency = ConversationCacheRecency(capacity: 2)
        _ = recency.touch("active")
        _ = recency.touch("old")

        #expect(recency.touch("new", protected: ["active", "new"]) == ["old"])
        #expect(recency.sessionIDs == ["active", "new"])
    }

    @Test("thirty conversation visits retain only the eight most recent")
    func thirtyConversationVisitsStayBounded() {
        var recency = ConversationCacheRecency(capacity: 8)
        var evicted: [String] = []
        for index in 0..<30 {
            evicted.append(contentsOf: recency.touch("session-\(index)"))
        }

        #expect(recency.sessionIDs.count == 8)
        #expect(recency.sessionIDs == (22..<30).map { "session-\($0)" })
        #expect(evicted == (0..<22).map { "session-\($0)" })
    }

    @Test("visual session polling backs off when idle")
    func visualSessionPollingBacksOff() {
        #expect(VisualSessionPollingPolicy.interval(
            hasSessions: false,
            hasSelectedConversation: true
        ) == .seconds(5))
        #expect(VisualSessionPollingPolicy.interval(
            hasSessions: true,
            hasSelectedConversation: false
        ) == .milliseconds(1_500))
        #expect(VisualSessionPollingPolicy.interval(
            hasSessions: true,
            hasSelectedConversation: true
        ) == .milliseconds(450))
    }

    @Test("clipboard polling backs off and stays slower in background")
    func clipboardPollingBacksOff() {
        #expect(ClipboardPollingPolicy.interval(
            isApplicationActive: true,
            idlePollCount: 0
        ) == .milliseconds(300))
        #expect(ClipboardPollingPolicy.interval(
            isApplicationActive: true,
            idlePollCount: 20
        ) == .milliseconds(1_500))
        #expect(ClipboardPollingPolicy.interval(
            isApplicationActive: false,
            idlePollCount: 20
        ) == .seconds(3))
    }
}
