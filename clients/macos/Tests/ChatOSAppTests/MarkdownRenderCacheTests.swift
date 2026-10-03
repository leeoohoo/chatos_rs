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
    func onlyLongInlineMarkdownGetsItsOwnBoundedViewport() {
        #expect(!MarkdownLayoutPolicy.shouldUseBoundedViewport("**Short** reply"))
        #expect(MarkdownLayoutPolicy.shouldUseBoundedViewport(
            String(repeating: "long task result line\n", count: 80)
        ))
    }

    @Test
    func onlyIncrementalStreamingMarkdownUsesTheUpdateDebounce() {
        #expect(MarkdownLayoutPolicy.shouldDebounceStreamingUpdate(
            previousSource: "Partial reply",
            nextSource: "Partial reply with another token"
        ))
        #expect(!MarkdownLayoutPolicy.shouldDebounceStreamingUpdate(
            previousSource: "Old document",
            nextSource: "Completely different document"
        ))
        #expect(!MarkdownLayoutPolicy.shouldDebounceStreamingUpdate(
            previousSource: "Completed reply",
            nextSource: "Completed reply"
        ))
    }

    @Test
    func remoteMarkdownImagesAreValidatedAndBoundedBeforeDecoding() {
        #expect(MarkdownRemoteImageLoader.allowedURL(
            from: "https://example.test/api/attachments/object?token=signed"
        ) != nil)
        #expect(MarkdownRemoteImageLoader.allowedURL(
            from: "https://example.test/api/attachments/object"
        ) == nil)
        #expect(MarkdownRemoteImageLoader.allowedURL(
            from: "https://example.test/untrusted.png?token=signed"
        ) == nil)
        #expect(MarkdownRemoteImageLoader.boundedDisplaySize(
            pixelWidth: 4_000,
            pixelHeight: 2_000
        ) == NSSize(width: 520, height: 260))
        #expect(MarkdownRemoteImageLoader.boundedDisplaySize(
            pixelWidth: 100,
            pixelHeight: 50
        ) == NSSize(width: 100, height: 50))
        #expect(MarkdownRemoteImageLoader.boundedDisplaySize(
            pixelWidth: 20_000,
            pixelHeight: 20_000
        ) == nil)
    }

    @Test
    func remoteMarkdownImageRequestsAreDeduplicatedAndCapped() {
        let values = (0..<40).map { index in
            "https://example.test/api/attachments/object?token=signed-\(index)"
        }
        let blocks = values.map { MarkdownBlock.image(altText: "image", url: $0) }
            + [.image(altText: "duplicate", url: values[0])]

        let requests = MarkdownRemoteImageLoader.requests(from: blocks)

        #expect(requests.count == MarkdownRemoteImageLoader.maximumImagesPerDocument)
        #expect(Set(requests.map(\.rawValue)).count == requests.count)
        #expect(requests.first?.rawValue == values[0])
        #expect(requests.last?.rawValue == values[31])
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
    func markdownOccurrenceCountingIsCaseInsensitiveAndNonAllocatingByMatch() {
        #expect(MarkdownOccurrenceCounter.count(
            in: "Agent result agent RESULT Agent",
            query: "agent"
        ) == 3)
        #expect(MarkdownOccurrenceCounter.count(in: "aaaa", query: "aa") == 2)
        #expect(MarkdownOccurrenceCounter.count(in: "content", query: "") == 0)
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
