import CoreGraphics
import Testing
@testable import ChatOSApp

struct AgentChatMentionTests {
    @Test
    func trailingAtSignProducesSearchQuery() throws {
        let query = try #require(AgentChatMentionSyntax.trailingQuery(in: "请处理一下 @玄德"))
        #expect(query.value == "玄德")
        #expect(AgentChatMentionSyntax.removingTrailingQuery(query, from: "请处理一下 @玄德") == "请处理一下 ")
    }

    @Test
    func emailAndClosedMentionDoNotOpenSuggestions() {
        #expect(AgentChatMentionSyntax.trailingQuery(in: "mail@example") == nil)
        #expect(AgentChatMentionSyntax.trailingQuery(in: "@玄德 请处理") == nil)
    }

    @Test
    func exactTypedMentionHonorsFollowingBoundary() {
        #expect(AgentChatMentionSyntax.containsMention(named: "玄德", in: "@玄德，请处理一下"))
        #expect(AgentChatMentionSyntax.containsMention(named: "玄德", in: "请 @玄德") == true)
        #expect(AgentChatMentionSyntax.containsMention(named: "玄德", in: "@玄德二号") == false)
    }
}

struct AgentChatTimelineScrollTests {
    @Test
    func bottomDetectionChangesOnlyWhenMarkerLeavesViewportTolerance() {
        #expect(AgentChatTimelineScrollMetrics.isAtBottom(
            markerMaxY: 600,
            viewportHeight: 600
        ))
        #expect(AgentChatTimelineScrollMetrics.isAtBottom(
            markerMaxY: 631,
            viewportHeight: 600
        ))
        #expect(!AgentChatTimelineScrollMetrics.isAtBottom(
            markerMaxY: 633,
            viewportHeight: 600
        ))
        #expect(!AgentChatTimelineScrollMetrics.isAtBottom(
            markerMaxY: CGFloat.greatestFiniteMagnitude,
            viewportHeight: 600
        ))
    }
}
