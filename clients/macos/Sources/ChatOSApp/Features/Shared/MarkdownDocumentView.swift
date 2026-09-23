import Foundation
import AppKit
import SwiftUI

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
            case .code, .divider:
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

struct MarkdownDocumentView: View {
    enum WidthBehavior: Equatable {
        case fill
        case fitContent
    }

    private struct LoadedDocument {
        let source: String
        let blocks: [MarkdownBlock]
    }

    private let markdown: String
    private let allowsTextSelection: Bool
    private let widthBehavior: WidthBehavior
    private let viewport: MarkdownViewport

    @State private var loadedDocument: LoadedDocument?

    init(
        markdown: String,
        allowsTextSelection: Bool = true,
        widthBehavior: WidthBehavior = .fill,
        maximumHeight: CGFloat = MarkdownLayoutPolicy.maximumInlineHeight
    ) {
        self.markdown = markdown
        self.allowsTextSelection = allowsTextSelection
        self.widthBehavior = widthBehavior
        viewport = .bounded(maximumHeight: maximumHeight)

        let initialBlocks: [MarkdownBlock]?
        if let cached = MarkdownRenderCache.shared.cachedBlocks(for: markdown) {
            initialBlocks = cached
        } else if MarkdownLayoutPolicy.shouldParseOffMain(markdown) {
            initialBlocks = nil
        } else {
            initialBlocks = MarkdownRenderCache.shared.blocks(for: markdown)
        }
        _loadedDocument = State(
            initialValue: initialBlocks.map { LoadedDocument(source: markdown, blocks: $0) }
        )
    }

    private init(
        markdown: String,
        allowsTextSelection: Bool = true,
        widthBehavior: WidthBehavior = .fill,
        viewport: MarkdownViewport
    ) {
        self.markdown = markdown
        self.allowsTextSelection = allowsTextSelection
        self.widthBehavior = widthBehavior
        self.viewport = viewport

        let initialBlocks = MarkdownRenderCache.shared.cachedBlocks(for: markdown)
        _loadedDocument = State(
            initialValue: initialBlocks.map { LoadedDocument(source: markdown, blocks: $0) }
        )
    }

    var body: some View {
        Group {
            if let loadedDocument, loadedDocument.source == markdown {
                if viewport.fillsAvailableHeight
                    || MarkdownLayoutPolicy.shouldUseBoundedViewport(markdown) {
                    MarkdownNativeScrollView(
                        source: markdown,
                        blocks: loadedDocument.blocks,
                        allowsTextSelection: allowsTextSelection,
                        widthBehavior: widthBehavior,
                        viewport: viewport
                    )
                    .frame(
                        maxWidth: widthBehavior == .fill ? .infinity : nil,
                        maxHeight: viewport.fillsAvailableHeight ? .infinity : nil,
                        alignment: .leading
                    )
                } else {
                    MarkdownNativeTextView(
                        source: markdown,
                        blocks: loadedDocument.blocks,
                        allowsTextSelection: allowsTextSelection,
                        widthBehavior: widthBehavior
                    )
                    .frame(
                        maxWidth: widthBehavior == .fill ? .infinity : nil,
                        alignment: .leading
                    )
                }
            } else {
                markdownLoadingPlaceholder
            }
        }
        .task(id: markdown) {
            guard loadedDocument?.source != markdown else { return }
            let source = markdown
            let parsed = await Task.detached(priority: .userInitiated) {
                let blocks = MarkdownRenderCache.shared.blocks(for: source)
                MarkdownRenderCache.shared.prepareInlineAttributes(for: blocks)
                return blocks
            }.value
            guard !Task.isCancelled, source == markdown else { return }
            loadedDocument = LoadedDocument(source: source, blocks: parsed)
        }
    }

    @ViewBuilder
    private var markdownLoadingPlaceholder: some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small)
            Text("正在解析 Markdown…")
                .appFont(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(
            maxWidth: .infinity,
            minHeight: viewport.fillsAvailableHeight ? nil : 64,
            maxHeight: viewport.fillsAvailableHeight ? .infinity : nil,
            alignment: .topLeading
        )
    }

    fileprivate static func reader(
        markdown: String,
        allowsTextSelection: Bool
    ) -> Self {
        Self(
            markdown: markdown,
            allowsTextSelection: allowsTextSelection,
            viewport: .reader
        )
    }

    fileprivate static func deferred(
        markdown: String,
        allowsTextSelection: Bool,
        maximumHeight: CGFloat
    ) -> Self {
        Self(
            markdown: markdown,
            allowsTextSelection: allowsTextSelection,
            viewport: .bounded(maximumHeight: maximumHeight)
        )
    }
}

struct DeferredMarkdownDocumentView: View {
    let markdown: String
    var allowsTextSelection = true
    var maximumHeight: CGFloat = MarkdownLayoutPolicy.maximumInlineHeight

    var body: some View {
        MarkdownDocumentView.deferred(
            markdown: markdown,
            allowsTextSelection: allowsTextSelection,
            maximumHeight: maximumHeight
        )
    }
}

/// A full-document reader owns its viewport and scrolling in AppKit. SwiftUI receives a stable
/// rectangle instead of repeatedly measuring the entire document inside a lazy stack.
struct MarkdownReaderView: View {
    let markdown: String
    var allowsTextSelection = true

    var body: some View {
        MarkdownDocumentView.reader(
            markdown: markdown,
            allowsTextSelection: allowsTextSelection
        )
    }
}

enum MarkdownLayoutPolicy {
    static let maximumInlineHeight: CGFloat = 520
    static let backgroundParsingByteThreshold = 8 * 1_024
    static let backgroundParsingLineThreshold = 120
    static let boundedViewportByteThreshold = 1_500
    static let boundedViewportLineThreshold = 32

    static func shouldParseOffMain(_ source: String) -> Bool {
        source.utf8.count >= backgroundParsingByteThreshold
            || source.lazy.filter(\.isNewline).prefix(backgroundParsingLineThreshold).count
                >= backgroundParsingLineThreshold
    }

    static func shouldUseBoundedViewport(_ source: String) -> Bool {
        source.utf8.count >= boundedViewportByteThreshold
            || source.lazy.filter(\.isNewline).prefix(boundedViewportLineThreshold).count
                >= boundedViewportLineThreshold
    }
}

enum MarkdownViewport: Equatable {
    case bounded(maximumHeight: CGFloat)
    case reader

    var fillsAvailableHeight: Bool { self == .reader }
}

/// A single AppKit text layout per Markdown document. SwiftUI's selectable `Text` creates a
/// `SelectionOverlay` for each rendered fragment. On macOS 26, a document containing many
/// fragments (especially a fenced SVG block) can enter an AttributeGraph invalidation loop.
/// NSTextView owns selection and wrapping without involving those overlays.
private struct MarkdownNativeTextView: NSViewRepresentable {
    let source: String
    let blocks: [MarkdownBlock]
    let allowsTextSelection: Bool
    let widthBehavior: MarkdownDocumentView.WidthBehavior

    func makeNSView(context: Context) -> MarkdownLayoutTextView {
        MarkdownLayoutTextView()
    }

    func updateNSView(_ textView: MarkdownLayoutTextView, context: Context) {
        textView.setDocument(
            source: source,
            blocks: blocks,
            allowsTextSelection: allowsTextSelection
        )
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView textView: MarkdownLayoutTextView,
        context: Context
    ) -> CGSize? {
        guard let proposedWidth = MarkdownLayoutGeometry.finitePositiveWidth(proposal.width) else {
            return nil
        }
        let width = switch widthBehavior {
        case .fill: proposedWidth
        case .fitContent: textView.width(fittingMaxWidth: proposedWidth)
        }
        return CGSize(width: width, height: textView.height(fittingWidth: width))
    }
}

private struct MarkdownNativeScrollView: NSViewRepresentable {
    let source: String
    let blocks: [MarkdownBlock]
    let allowsTextSelection: Bool
    let widthBehavior: MarkdownDocumentView.WidthBehavior
    let viewport: MarkdownViewport

    func makeNSView(context: Context) -> MarkdownScrollContainerView {
        MarkdownScrollContainerView()
    }

    func updateNSView(_ scrollView: MarkdownScrollContainerView, context: Context) {
        scrollView.setDocument(
            source: source,
            blocks: blocks,
            allowsTextSelection: allowsTextSelection
        )
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView scrollView: MarkdownScrollContainerView,
        context: Context
    ) -> CGSize? {
        guard let proposedWidth = MarkdownLayoutGeometry.finitePositiveWidth(proposal.width) else {
            return nil
        }
        let width = switch widthBehavior {
        case .fill: proposedWidth
        case .fitContent: scrollView.textWidth(fittingMaxWidth: proposedWidth)
        }
        let contentHeight = scrollView.textHeight(fittingWidth: width)
        let height = MarkdownLayoutGeometry.resolvedHeight(
            contentHeight: contentHeight,
            proposedHeight: proposal.height,
            viewport: viewport
        )
        return CGSize(width: width, height: height)
    }
}

enum MarkdownLayoutGeometry {
    static func finitePositiveWidth(_ width: CGFloat?) -> CGFloat? {
        guard let width, width.isFinite, width > 0 else { return nil }
        return width
    }

    static func widthCacheKey(fittingWidth width: CGFloat) -> UInt64? {
        guard let width = finitePositiveWidth(width) else { return nil }
        return Double(max(width, 1)).bitPattern
    }

    static func resolvedHeight(
        contentHeight: CGFloat,
        proposedHeight: CGFloat?,
        viewport: MarkdownViewport
    ) -> CGFloat {
        let safeContentHeight = finitePositiveWidth(contentHeight) ?? 1
        switch viewport {
        case let .bounded(maximumHeight):
            let safeMaximumHeight = finitePositiveWidth(maximumHeight)
                ?? MarkdownLayoutPolicy.maximumInlineHeight
            return min(safeContentHeight, safeMaximumHeight)
        case .reader:
            return finitePositiveWidth(proposedHeight)
                ?? min(safeContentHeight, MarkdownLayoutPolicy.maximumInlineHeight)
        }
    }
}

@MainActor
private final class MarkdownScrollContainerView: NSScrollView {
    private let markdownTextView = MarkdownLayoutTextView()

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        drawsBackground = false
        borderType = .noBorder
        hasHorizontalScroller = false
        hasVerticalScroller = true
        autohidesScrollers = true
        scrollerStyle = .overlay
        horizontalScrollElasticity = .none
        verticalScrollElasticity = .automatic
        automaticallyAdjustsContentInsets = false
        contentInsets = NSEdgeInsets(top: 0, left: 0, bottom: 0, right: 0)
        documentView = markdownTextView

        markdownTextView.minSize = .zero
        markdownTextView.maxSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        markdownTextView.isHorizontallyResizable = false
        markdownTextView.isVerticallyResizable = true
        markdownTextView.autoresizingMask = [.width]
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layout() {
        super.layout()
        let width = max(contentSize.width, 1)
        let height = max(markdownTextView.height(fittingWidth: width), contentSize.height)
        let nextSize = NSSize(width: width, height: height)
        if abs(markdownTextView.frame.width - nextSize.width) > 0.5
            || abs(markdownTextView.frame.height - nextSize.height) > 0.5 {
            markdownTextView.setFrameSize(nextSize)
        }
    }

    func setDocument(
        source: String,
        blocks: [MarkdownBlock],
        allowsTextSelection: Bool
    ) {
        markdownTextView.setDocument(
            source: source,
            blocks: blocks,
            allowsTextSelection: allowsTextSelection
        )
        needsLayout = true
    }

    func textHeight(fittingWidth width: CGFloat) -> CGFloat {
        markdownTextView.height(fittingWidth: width)
    }

    func textWidth(fittingMaxWidth maxWidth: CGFloat) -> CGFloat {
        markdownTextView.width(fittingMaxWidth: maxWidth)
    }
}

@MainActor
private final class MarkdownLayoutTextView: NSTextView {
    private var source = ""
    private var measuredHeights: [UInt64: CGFloat] = [:]
    private var measuredWidths: [UInt64: CGFloat] = [:]

    init() {
        let storage = NSTextStorage()
        let layoutManager = NSLayoutManager()
        let container = NSTextContainer(
            containerSize: NSSize(width: 0, height: CGFloat.greatestFiniteMagnitude)
        )
        storage.addLayoutManager(layoutManager)
        layoutManager.addTextContainer(container)
        super.init(frame: .zero, textContainer: container)

        drawsBackground = false
        isEditable = false
        isSelectable = true
        isRichText = true
        importsGraphics = false
        allowsUndo = false
        textContainerInset = .zero
        container.lineFragmentPadding = 0
        container.widthTracksTextView = true
        container.heightTracksTextView = false
        isHorizontallyResizable = false
        isVerticallyResizable = true
        autoresizingMask = [.width]
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func setDocument(
        source nextSource: String,
        blocks: [MarkdownBlock],
        allowsTextSelection: Bool
    ) {
        isSelectable = allowsTextSelection
        guard source != nextSource else { return }
        source = nextSource
        textStorage?.setAttributedString(MarkdownAttributedRenderer.render(blocks))
        measuredHeights.removeAll(keepingCapacity: true)
        measuredWidths.removeAll(keepingCapacity: true)
        invalidateIntrinsicContentSize()
    }

    func height(fittingWidth width: CGFloat) -> CGFloat {
        guard let safeWidth = MarkdownLayoutGeometry.finitePositiveWidth(width),
              let widthKey = MarkdownLayoutGeometry.widthCacheKey(fittingWidth: safeWidth) else {
            return 1
        }
        if let cached = measuredHeights[widthKey] { return cached }
        // Measurement must be pure. Mutating NSTextContainer from NSViewRepresentable's
        // sizeThatFits invalidates the platform view while SwiftUI is placing a lazy stack,
        // producing an endless size/place cycle for very tall messages.
        let measured = textStorage?.boundingRect(
            with: NSSize(width: safeWidth, height: CGFloat.greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin, .usesFontLeading]
        ).height ?? 1
        let result = max(ceil(measured), 1)
        measuredHeights[widthKey] = result
        return result
    }

    func width(fittingMaxWidth maxWidth: CGFloat) -> CGFloat {
        guard let safeMaxWidth = MarkdownLayoutGeometry.finitePositiveWidth(maxWidth),
              let widthKey = MarkdownLayoutGeometry.widthCacheKey(fittingWidth: safeMaxWidth) else {
            return 1
        }
        if let cached = measuredWidths[widthKey] { return cached }
        let measured = textStorage?.boundingRect(
            with: NSSize(width: safeMaxWidth, height: CGFloat.greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin, .usesFontLeading]
        ).width ?? 1
        let result = min(max(ceil(measured), 1), safeMaxWidth)
        measuredWidths[widthKey] = result
        return result
    }
}

@MainActor
private enum MarkdownAttributedRenderer {
    private static let inlineIntentKey = NSAttributedString.Key("NSInlinePresentationIntent")

    static func render(_ blocks: [MarkdownBlock]) -> NSAttributedString {
        let result = NSMutableAttributedString()
        for (index, block) in blocks.enumerated() {
            if index > 0 { result.append(NSAttributedString(string: "\n\n")) }
            append(block, to: result)
        }
        return result
    }

    private static func append(_ block: MarkdownBlock, to result: NSMutableAttributedString) {
        switch block {
        case let .heading(level, text):
            let size: CGFloat = switch level {
            case 1: 20
            case 2: 17
            case 3: 15
            default: 13
            }
            result.append(inline(text, font: .systemFont(ofSize: size, weight: .bold)))

        case let .paragraph(text):
            result.append(inline(text, font: .systemFont(ofSize: 13)))

        case let .list(items):
            for (index, item) in items.enumerated() {
                if index > 0 { result.append(NSAttributedString(string: "\n")) }
                let indent = String(repeating: "    ", count: item.depth)
                result.append(NSAttributedString(
                    string: "\(indent)\(item.marker) ",
                    attributes: baseAttributes(font: .systemFont(ofSize: 13, weight: .semibold))
                ))
                result.append(inline(item.text, font: .systemFont(ofSize: 13)))
            }

        case let .quote(text):
            result.append(NSAttributedString(
                string: "▎ ",
                attributes: baseAttributes(
                    font: .systemFont(ofSize: 13, weight: .semibold),
                    color: .secondaryLabelColor
                )
            ))
            result.append(inline(
                text,
                font: .systemFont(ofSize: 13),
                color: .secondaryLabelColor
            ))

        case let .code(language, content):
            if let language, !language.isEmpty {
                result.append(NSAttributedString(
                    string: language.uppercased() + "\n",
                    attributes: baseAttributes(
                        font: .systemFont(ofSize: 10, weight: .semibold),
                        color: .secondaryLabelColor
                    )
                ))
            }
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineSpacing = 3
            paragraph.paragraphSpacingBefore = 6
            paragraph.paragraphSpacing = 6
            result.append(NSAttributedString(
                string: content,
                attributes: [
                    .font: NSFont.monospacedSystemFont(ofSize: 12, weight: .regular),
                    .foregroundColor: NSColor.labelColor,
                    .backgroundColor: NSColor.textBackgroundColor.withAlphaComponent(0.65),
                    .paragraphStyle: paragraph,
                ]
            ))

        case .divider:
            result.append(NSAttributedString(
                string: "────────────────────────",
                attributes: baseAttributes(font: .systemFont(ofSize: 10), color: .separatorColor)
            ))

        case let .table(headers, rows):
            let values = [headers] + rows
            for (index, row) in values.enumerated() {
                if index > 0 { result.append(NSAttributedString(string: "\n")) }
                result.append(NSAttributedString(
                    string: row.joined(separator: "  │  "),
                    attributes: baseAttributes(
                        font: .monospacedSystemFont(
                            ofSize: 12,
                            weight: index == 0 ? .semibold : .regular
                        )
                    )
                ))
                if index == 0 {
                    result.append(NSAttributedString(
                        string: "\n" + String(repeating: "─", count: max(row.count * 10, 10)),
                        attributes: baseAttributes(
                            font: .monospacedSystemFont(ofSize: 12, weight: .regular),
                            color: .separatorColor
                        )
                    ))
                }
            }
        }
    }

    private static func inline(
        _ source: String,
        font: NSFont,
        color: NSColor = .labelColor
    ) -> NSAttributedString {
        let rendered = NSAttributedString(MarkdownRenderCache.shared.attributedInline(for: source))
        let result = NSMutableAttributedString(attributedString: rendered)
        let fullRange = NSRange(location: 0, length: result.length)
        result.addAttributes(baseAttributes(font: font, color: color), range: fullRange)
        rendered.enumerateAttribute(inlineIntentKey, in: fullRange) { value, range, _ in
            guard let rawValue = (value as? NSNumber)?.intValue else { return }
            var resolvedFont = font
            if rawValue & 4 != 0 {
                resolvedFont = .monospacedSystemFont(ofSize: font.pointSize, weight: .regular)
                result.addAttribute(
                    .backgroundColor,
                    value: NSColor.textBackgroundColor.withAlphaComponent(0.65),
                    range: range
                )
            } else {
                if rawValue & 2 != 0 {
                    resolvedFont = NSFontManager.shared.convert(resolvedFont, toHaveTrait: .boldFontMask)
                }
                if rawValue & 1 != 0 {
                    resolvedFont = NSFontManager.shared.convert(resolvedFont, toHaveTrait: .italicFontMask)
                }
            }
            result.addAttribute(.font, value: resolvedFont, range: range)
            if rawValue & 8 != 0 {
                result.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: range)
            }
        }
        return result
    }

    private static func baseAttributes(
        font: NSFont,
        color: NSColor = .labelColor
    ) -> [NSAttributedString.Key: Any] {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = 3
        return [
            .font: font,
            .foregroundColor: color,
            .paragraphStyle: paragraph,
        ]
    }
}

extension View {
    @ViewBuilder
    func appTextSelection(_ isEnabled: Bool) -> some View {
        if isEnabled {
            textSelection(.enabled)
        } else {
            self
        }
    }
}
