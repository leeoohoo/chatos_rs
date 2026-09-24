import Foundation
import AppKit
import SwiftUI
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
    private static let imageCache = NSCache<NSString, NSImage>()
    private var source = ""
    private var measuredHeights: [UInt64: CGFloat] = [:]
    private var measuredWidths: [UInt64: CGFloat] = [:]
    private var imageLoadTask: Task<Void, Never>?

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

    deinit {
        imageLoadTask?.cancel()
    }

    func setDocument(
        source nextSource: String,
        blocks: [MarkdownBlock],
        allowsTextSelection: Bool
    ) {
        isSelectable = allowsTextSelection
        guard source != nextSource else { return }
        imageLoadTask?.cancel()
        source = nextSource
        textStorage?.setAttributedString(MarkdownAttributedRenderer.render(blocks))
        invalidateMeasurements()

        let imageBlocks = blocks.compactMap { block -> (String, URL)? in
            guard case let .image(_, rawURL) = block,
                  let url = MarkdownRemoteImageLoader.allowedURL(from: rawURL) else { return nil }
            return (rawURL, url)
        }
        guard !imageBlocks.isEmpty else { return }
        imageLoadTask = Task { [weak self] in
            guard let self else { return }
            var images: [String: NSImage] = [:]
            for (rawURL, url) in imageBlocks {
                guard !Task.isCancelled else { return }
                if let cached = Self.imageCache.object(forKey: rawURL as NSString) {
                    images[rawURL] = cached
                    continue
                }
                guard let data = await MarkdownRemoteImageLoader.load(url),
                      !Task.isCancelled,
                      let image = NSImage(data: data) else { continue }
                Self.imageCache.setObject(image, forKey: rawURL as NSString, cost: data.count)
                images[rawURL] = image
            }
            guard !Task.isCancelled, source == nextSource, !images.isEmpty else { return }
            textStorage?.setAttributedString(
                MarkdownAttributedRenderer.render(blocks, loadedImages: images)
            )
            invalidateMeasurements()
            enclosingScrollView?.needsLayout = true
        }
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

    private func invalidateMeasurements() {
        measuredHeights.removeAll(keepingCapacity: true)
        measuredWidths.removeAll(keepingCapacity: true)
        invalidateIntrinsicContentSize()
        needsDisplay = true
    }
}

private enum MarkdownRemoteImageLoader {
    static let maximumBytes = 10 * 1_024 * 1_024

    static func allowedURL(from rawValue: String) -> URL? {
        guard let url = URL(string: rawValue),
              matchesAllowedScheme(url.scheme),
              isChatOSAttachmentPath(url.path),
              URLComponents(url: url, resolvingAgainstBaseURL: false)?
                .queryItems?.contains(where: { $0.name == "token" && !($0.value ?? "").isEmpty }) == true
        else { return nil }
        return url
    }

    static func load(_ url: URL) async -> Data? {
        var request = URLRequest(url: url)
        request.timeoutInterval = 20
        guard let (data, response) = try? await URLSession.shared.data(for: request),
              !data.isEmpty,
              data.count <= maximumBytes,
              let response = response as? HTTPURLResponse,
              (200..<300).contains(response.statusCode),
              response.mimeType?.hasPrefix("image/") == true else { return nil }
        return data
    }

    private static func matchesAllowedScheme(_ scheme: String?) -> Bool {
        scheme?.lowercased() == "https" || scheme?.lowercased() == "http"
    }

    private static func isChatOSAttachmentPath(_ path: String) -> Bool {
        path == "/api/attachments/object"
            || path.hasSuffix("/attachments/object")
    }
}

@MainActor
enum MarkdownAttributedRenderer {
    private static let inlineIntentKey = NSAttributedString.Key("NSInlinePresentationIntent")

    static func render(
        _ blocks: [MarkdownBlock],
        loadedImages: [String: NSImage] = [:]
    ) -> NSAttributedString {
        let result = NSMutableAttributedString()
        for (index, block) in blocks.enumerated() {
            if index > 0 { result.append(NSAttributedString(string: "\n\n")) }
            append(block, loadedImages: loadedImages, to: result)
        }
        return result
    }

    private static func append(
        _ block: MarkdownBlock,
        loadedImages: [String: NSImage],
        to result: NSMutableAttributedString
    ) {
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

        case let .image(altText, url):
            if let image = loadedImages[url] {
                let attachment = NSTextAttachment()
                attachment.image = image
                let maximumSize = NSSize(width: 520, height: 420)
                let naturalSize = image.size
                let scale = min(
                    1,
                    min(
                        maximumSize.width / max(naturalSize.width, 1),
                        maximumSize.height / max(naturalSize.height, 1)
                    )
                )
                attachment.bounds = NSRect(
                    origin: .zero,
                    size: NSSize(
                        width: max(1, naturalSize.width * scale),
                        height: max(1, naturalSize.height * scale)
                    )
                )
                result.append(NSAttributedString(attachment: attachment))
            } else {
                let label = altText.isEmpty ? "图片" : altText
                let placeholder = NSMutableAttributedString(
                    string: "🖼 \(label)",
                    attributes: baseAttributes(
                        font: .systemFont(ofSize: 13, weight: .medium),
                        color: .secondaryLabelColor
                    )
                )
                if let link = URL(string: url) {
                    placeholder.addAttribute(
                        .link,
                        value: link,
                        range: NSRange(location: 0, length: placeholder.length)
                    )
                }
                result.append(placeholder)
            }

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
            appendTable(headers: headers, rows: rows, to: result)
        }
    }

    private static func appendTable(
        headers: [String],
        rows: [[String]],
        to result: NSMutableAttributedString
    ) {
        let columnCount = max(1, headers.count)
        let table = NSTextTable()
        table.numberOfColumns = columnCount
        table.layoutAlgorithm = .fixedLayoutAlgorithm
        table.collapsesBorders = true
        table.hidesEmptyCells = false
        table.setContentWidth(100, type: .percentageValueType)

        let values = [headers] + rows
        for (rowIndex, rawRow) in values.enumerated() {
            let row = normalizedTableRow(rawRow, columnCount: columnCount)
            for columnIndex in 0..<columnCount {
                let block = NSTextTableBlock(
                    table: table,
                    startingRow: rowIndex,
                    rowSpan: 1,
                    startingColumn: columnIndex,
                    columnSpan: 1
                )
                block.verticalAlignment = .topAlignment
                block.setWidth(6, type: .absoluteValueType, for: .padding)
                block.setWidth(0.5, type: .absoluteValueType, for: .border)
                block.setBorderColor(NSColor.separatorColor.withAlphaComponent(0.7))
                if rowIndex == 0 {
                    block.backgroundColor = NSColor.controlBackgroundColor.withAlphaComponent(0.82)
                } else if rowIndex.isMultiple(of: 2) {
                    block.backgroundColor = NSColor.controlBackgroundColor.withAlphaComponent(0.28)
                }

                let paragraph = NSMutableParagraphStyle()
                paragraph.textBlocks = [block]
                paragraph.lineSpacing = 2
                paragraph.paragraphSpacing = 0
                let font = NSFont.systemFont(
                    ofSize: 12,
                    weight: rowIndex == 0 ? .semibold : .regular
                )
                let cell = NSMutableAttributedString(
                    attributedString: inline(row[columnIndex], font: font)
                )
                if cell.length == 0 {
                    cell.append(NSAttributedString(string: " "))
                }
                cell.append(NSAttributedString(string: "\n"))
                cell.addAttribute(
                    .paragraphStyle,
                    value: paragraph,
                    range: NSRange(location: 0, length: cell.length)
                )
                result.append(cell)
            }
        }
    }

    private static func normalizedTableRow(
        _ row: [String],
        columnCount: Int
    ) -> [String] {
        if row.count == columnCount { return row }
        if row.count < columnCount {
            return row + Array(repeating: "", count: columnCount - row.count)
        }
        return Array(row.prefix(columnCount - 1))
            + [row.dropFirst(columnCount - 1).joined(separator: " | ")]
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
