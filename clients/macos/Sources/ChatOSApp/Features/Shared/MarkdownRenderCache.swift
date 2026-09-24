import Foundation

struct MarkdownRenderCacheMetrics: Equatable {
    var blockHits = 0
    var blockMisses = 0
    var inlineHits = 0
    var inlineMisses = 0
}

/// Markdown appears in several frequently refreshed SwiftUI surfaces. Keeping the parsed form
/// here avoids reparsing every visible message whenever unrelated view state changes.
final class MarkdownRenderCache: @unchecked Sendable {
    static let shared = MarkdownRenderCache()

    private final class BlockEntry {
        let value: [MarkdownBlock]

        init(_ value: [MarkdownBlock]) {
            self.value = value
        }
    }

    private final class InlineEntry {
        let value: AttributedString

        init(_ value: AttributedString) {
            self.value = value
        }
    }

    private let blockCache = NSCache<NSString, BlockEntry>()
    private let inlineCache = NSCache<NSString, InlineEntry>()
    private let metricsLock = NSLock()
    private var storedMetrics = MarkdownRenderCacheMetrics()

    init(totalCostLimit: Int = 16 * 1_024 * 1_024, countLimit: Int = 128) {
        // Split the budget between document structure and rendered inline text. NSCache can
        // discard either half under memory pressure and never turns chat history into an
        // unbounded in-memory copy.
        blockCache.totalCostLimit = totalCostLimit / 2
        inlineCache.totalCostLimit = totalCostLimit / 2
        blockCache.countLimit = max(countLimit / 2, 1)
        inlineCache.countLimit = max(countLimit / 2, 1)
    }

    func blocks(for source: String) -> [MarkdownBlock] {
        let key = source as NSString
        if let cached = blockCache.object(forKey: key) {
            updateMetrics { $0.blockHits += 1 }
            return cached.value
        }

        let parsed = MarkdownBlockParser.parse(source)
        blockCache.setObject(
            BlockEntry(parsed),
            forKey: key,
            cost: max(source.utf8.count, 1)
        )
        updateMetrics { $0.blockMisses += 1 }
        return parsed
    }

    func cachedBlocks(for source: String) -> [MarkdownBlock]? {
        let key = source as NSString
        guard let cached = blockCache.object(forKey: key) else { return nil }
        updateMetrics { $0.blockHits += 1 }
        return cached.value
    }

    func attributedInline(for source: String) -> AttributedString {
        let key = source as NSString
        if let cached = inlineCache.object(forKey: key) {
            updateMetrics { $0.inlineHits += 1 }
            return cached.value
        }

        let rendered = (try? AttributedString(
            markdown: source,
            options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        )) ?? AttributedString(source)
        inlineCache.setObject(
            InlineEntry(rendered),
            forKey: key,
            cost: max(source.utf8.count * 2, 1)
        )
        updateMetrics { $0.inlineMisses += 1 }
        return rendered
    }

    func prepareInlineAttributes(for blocks: [MarkdownBlock]) {
        for block in blocks {
            switch block {
            case let .heading(_, text), let .paragraph(text), let .quote(text):
                _ = attributedInline(for: text)
            case let .list(items):
                for item in items { _ = attributedInline(for: item.text) }
            case let .table(headers, rows):
                for cell in headers { _ = attributedInline(for: cell) }
                for row in rows {
                    for cell in row { _ = attributedInline(for: cell) }
                }
            case .image, .code, .divider:
                break
            }
        }
    }

    func metrics() -> MarkdownRenderCacheMetrics {
        metricsLock.lock()
        defer { metricsLock.unlock() }
        return storedMetrics
    }

    func removeAll() {
        blockCache.removeAllObjects()
        inlineCache.removeAllObjects()
        metricsLock.lock()
        storedMetrics = MarkdownRenderCacheMetrics()
        metricsLock.unlock()
    }

    private func updateMetrics(_ update: (inout MarkdownRenderCacheMetrics) -> Void) {
        metricsLock.lock()
        update(&storedMetrics)
        metricsLock.unlock()
    }
}
