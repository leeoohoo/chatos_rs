import Foundation
import AppKit
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

    @Test
    func parsesStandardAndModelStylePipeTables() {
        let standard = MarkdownBlockParser.parse("""
        | Source | 简体中文 |
        | --- | --- |
        | Security and quality | 安全与质量 |
        """)
        #expect(standard == [
            .table(
                headers: ["Source", "简体中文"],
                rows: [["Security and quality", "安全与质量"]]
            ),
        ])

        let withoutDelimiter = MarkdownBlockParser.parse("""
        Source | 简体中文
        Security and quality | 安全与质量
        Findings | 发现的问题
        Dependabot | Dependabot
        """)
        #expect(withoutDelimiter == [
            .table(
                headers: ["Source", "简体中文"],
                rows: [
                    ["Security and quality", "安全与质量"],
                    ["Findings", "发现的问题"],
                    ["Dependabot", "Dependabot"],
                ]
            ),
        ])
    }

    @Test
    func escapedAndInlineCodePipesStayInsideTableCells() {
        let blocks = MarkdownBlockParser.parse("""
        | Expression | Meaning |
        | --- | --- |
        | `a | b` | a \\| b |
        """)
        #expect(blocks == [
            .table(
                headers: ["Expression", "Meaning"],
                rows: [["`a | b`", "a \\| b"]]
            ),
        ])
    }

    @Test
    func parsesPlainAndAngleWrappedMarkdownImagesAsImageBlocks() {
        let blocks = MarkdownBlockParser.parse("""
        Before

        ![screenshot](<https://example.test/api/chatos/attachments/object?token=signed>)

        ![diagram](https://example.test/api/attachments/object?token=other)
        """)

        #expect(blocks == [
            .paragraph("Before"),
            .image(
                altText: "screenshot",
                url: "https://example.test/api/chatos/attachments/object?token=signed"
            ),
            .image(
                altText: "diagram",
                url: "https://example.test/api/attachments/object?token=other"
            ),
        ])
    }

    @Test @MainActor
    func tableRendererUsesNativeTextTableBlocks() {
        let rendered = MarkdownAttributedRenderer.render([
            .table(
                headers: ["Source", "简体中文"],
                rows: [["Security and quality", "安全与质量"]]
            ),
        ])
        var tableBlocks: [NSTextTableBlock] = []
        rendered.enumerateAttribute(
            .paragraphStyle,
            in: NSRange(location: 0, length: rendered.length)
        ) { value, _, _ in
            guard let style = value as? NSParagraphStyle else { return }
            tableBlocks.append(contentsOf: style.textBlocks.compactMap { $0 as? NSTextTableBlock })
        }

        #expect(tableBlocks.count == 4)
        #expect(Set(tableBlocks.map(\.startingColumn)) == Set([0, 1]))
        #expect(!rendered.string.contains("│"))
    }
}
