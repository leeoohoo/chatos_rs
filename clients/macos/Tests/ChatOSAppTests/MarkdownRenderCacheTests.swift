import Foundation
import Testing
@testable import ChatOSApp

struct MarkdownRenderCacheTests {
    @Test
    func markdownLayoutRejectsNonFiniteAndNonPositiveWidths() {
        #expect(MarkdownLayoutGeometry.finitePositiveWidth(nil) == nil)
        #expect(MarkdownLayoutGeometry.finitePositiveWidth(.infinity) == nil)
        #expect(MarkdownLayoutGeometry.finitePositiveWidth(-.infinity) == nil)
        #expect(MarkdownLayoutGeometry.finitePositiveWidth(.nan) == nil)
        #expect(MarkdownLayoutGeometry.finitePositiveWidth(0) == nil)
        #expect(MarkdownLayoutGeometry.finitePositiveWidth(-1) == nil)
        #expect(MarkdownLayoutGeometry.finitePositiveWidth(320) == 320)
    }

    @Test
    func markdownLayoutOnlyBuildsCacheKeysForFinitePositiveWidths() {
        #expect(MarkdownLayoutGeometry.widthCacheKey(fittingWidth: .infinity) == nil)
        #expect(MarkdownLayoutGeometry.widthCacheKey(fittingWidth: .nan) == nil)
        #expect(MarkdownLayoutGeometry.widthCacheKey(fittingWidth: 0) == nil)
        #expect(MarkdownLayoutGeometry.widthCacheKey(fittingWidth: 320) != nil)
    }

    @Test
    func inlineMarkdownHeightIsCappedButReaderUsesViewportHeight() {
        #expect(MarkdownLayoutGeometry.resolvedHeight(
            contentHeight: 1_800,
            proposedHeight: nil,
            viewport: .bounded(maximumHeight: 520)
        ) == 520)
        #expect(MarkdownLayoutGeometry.resolvedHeight(
            contentHeight: 1_800,
            proposedHeight: 640,
            viewport: .reader
        ) == 640)
        #expect(MarkdownLayoutGeometry.resolvedHeight(
            contentHeight: 240,
            proposedHeight: nil,
            viewport: .bounded(maximumHeight: 520)
        ) == 240)
    }

    @Test
    func largeMarkdownIsSelectedForBackgroundParsing() {
        #expect(!MarkdownLayoutPolicy.shouldParseOffMain("# Short\n\nBody"))
        #expect(MarkdownLayoutPolicy.shouldParseOffMain(
            String(repeating: "long markdown row\n", count: 600)
        ))
    }

    @Test
    func onlyLongInlineMarkdownGetsItsOwnBoundedViewport() {
        #expect(!MarkdownLayoutPolicy.shouldUseBoundedViewport("**Short** reply"))
        #expect(MarkdownLayoutPolicy.shouldUseBoundedViewport(
            String(repeating: "long task result line\n", count: 80)
        ))
    }

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

    @Test
    func megabyteMarkdownCanParseOffMainAndReuseBoundedCache() async {
        let cache = MarkdownRenderCache(totalCostLimit: 4 * 1_024 * 1_024, countLimit: 8)
        let row = "| column | value |\n| --- | --- |\n| key | a moderately long value |\n\n"
        let source = "# Large document\n\n" + String(repeating: row, count: 16_000)
        #expect(source.utf8.count > 1_000_000)

        let first = await Task.detached(priority: .userInitiated) {
            cache.blocks(for: source)
        }.value
        let second = await Task.detached(priority: .userInitiated) {
            cache.blocks(for: source)
        }.value

        #expect(!first.isEmpty)
        #expect(first == second)
        #expect(cache.metrics().blockMisses == 1)
        #expect(cache.metrics().blockHits == 1)
    }
}
