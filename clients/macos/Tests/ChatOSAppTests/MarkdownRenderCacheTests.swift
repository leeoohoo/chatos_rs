import Foundation
import Testing
@testable import ChatOSApp

struct MarkdownRenderCacheTests {
    @Test
    func repeatedDocumentParsingUsesBoundedCache() {
        let cache = MarkdownRenderCache(totalCostLimit: 1_024 * 1_024, countLimit: 16)
        let source = """
        # 标题

        - 第一项
        - 第二项

        ```svg
        <svg viewBox="0 0 10 10"><path d="M0 0L10 10"/></svg>
        ```
        """

        let first = cache.blocks(for: source)
        let second = cache.blocks(for: source)

        #expect(first == second)
        #expect(cache.metrics().blockMisses == 1)
        #expect(cache.metrics().blockHits == 1)
    }

    @Test
    func repeatedInlineMarkdownUsesCachedAttributedString() {
        let cache = MarkdownRenderCache(totalCostLimit: 1_024 * 1_024, countLimit: 16)
        let source = "**重点** 与 `代码`"

        let first = cache.attributedInline(for: source)
        let second = cache.attributedInline(for: source)

        #expect(first == second)
        #expect(cache.metrics().inlineMisses == 1)
        #expect(cache.metrics().inlineHits == 1)
    }
}
