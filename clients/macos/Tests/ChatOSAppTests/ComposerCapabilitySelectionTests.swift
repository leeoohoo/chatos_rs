import ChatOSCore
import Testing
@testable import ChatOSApp

struct ComposerCapabilitySelectionTests {
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
