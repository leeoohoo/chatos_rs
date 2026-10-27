import ChatOSCore
import Testing
@testable import ChatOSApp

struct ComposerCapabilitySelectionTests {
    @Test
    func capabilityMenuHasNoReservedHeightWithoutCandidates() {
        #expect(ComposerCapabilityMenuLayout.height(candidateCount: 0) == 0)
    }

    @Test
    func capabilityMenuExpandsToShowEverySmallResultSet() {
        #expect(ComposerCapabilityMenuLayout.height(candidateCount: 1) == 55)
        #expect(ComposerCapabilityMenuLayout.height(candidateCount: 3) == 169)
    }

    @Test
    func capabilityMenuCapsLargeResultSetsForScrolling() {
        #expect(ComposerCapabilityMenuLayout.height(candidateCount: 20) == 360)
    }

    @Test
    func bareSlashProducesEmptyCapabilityQuery() throws {
        let query = try #require(ComposerCapabilitySyntax.trailingQuery(in: "/"))
        #expect(query.value.isEmpty)
        #expect(ComposerCapabilitySyntax.removingTrailingQuery(query, from: "/") == "")
    }

    @Test
    func trailingSlashQueryCanFollowNaturalLanguageBoundary() throws {
        let query = try #require(ComposerCapabilitySyntax.trailingQuery(in: "请帮我处理 /Doc"))
        #expect(query.value == "Doc")
        #expect(ComposerCapabilitySyntax.removingTrailingQuery(query, from: "请帮我处理 /Doc") == "请帮我处理 ")
    }

    @Test
    func pathsAndClosedSelectionsDoNotOpenCapabilityMenu() {
        #expect(ComposerCapabilitySyntax.trailingQuery(in: "/Users/me/file") == nil)
        #expect(ComposerCapabilitySyntax.trailingQuery(in: "/Document 帮我处理") == nil)
        #expect(ComposerCapabilitySyntax.trailingQuery(in: "https://example.com/") == nil)
    }
}
