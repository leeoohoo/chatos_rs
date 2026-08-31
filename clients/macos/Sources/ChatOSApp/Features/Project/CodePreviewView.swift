import AppKit
import ChatOSCore
import SwiftUI

struct CodePreviewView: View {
    @Environment(\.interfaceFontScale) private var interfaceFontScale
    let content: String
    let fileName: String
    var targetLine: Int?
    var followsTail: Bool
    var onSymbolSelection: (ProjectCodeSymbolSelection?) -> Void

    init(
        content: String,
        fileName: String,
        targetLine: Int? = nil,
        followsTail: Bool = false,
        onSymbolSelection: @escaping (ProjectCodeSymbolSelection?) -> Void = { _ in }
    ) {
        self.content = content
        self.fileName = fileName
        self.targetLine = targetLine
        self.followsTail = followsTail
        self.onSymbolSelection = onSymbolSelection
    }

    var body: some View {
        NativeCodeTextView(
            text: .constant(content),
            fileName: fileName,
            isEditable: false,
            targetLine: targetLine,
            followsTail: followsTail,
            fontSize: CodeFontMetrics.fontSize(for: interfaceFontScale),
            onSymbolSelection: onSymbolSelection
        )
    }
}

struct CodeEditorView: View {
    @Environment(\.interfaceFontScale) private var interfaceFontScale
    @Binding var text: String
    let fileName: String
    var targetLine: Int?
    var onSymbolSelection: (ProjectCodeSymbolSelection?) -> Void = { _ in }

    var body: some View {
        NativeCodeTextView(
            text: $text,
            fileName: fileName,
            isEditable: true,
            targetLine: targetLine,
            followsTail: false,
            fontSize: CodeFontMetrics.fontSize(for: interfaceFontScale),
            onSymbolSelection: onSymbolSelection
        )
    }
}

private struct NativeCodeTextView: NSViewRepresentable {
    @Binding var text: String
    let fileName: String
    let isEditable: Bool
    let targetLine: Int?
    let followsTail: Bool
    let fontSize: CGFloat
    let onSymbolSelection: (ProjectCodeSymbolSelection?) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(parent: self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = CodeScrollView()
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.borderType = .noBorder
        scrollView.drawsBackground = true
        scrollView.backgroundColor = .textBackgroundColor
        scrollView.wantsLayer = true
        scrollView.layer?.masksToBounds = true

        let textView = NSTextView(frame: .zero)
        configure(textView)
        textView.delegate = context.coordinator
        scrollView.documentView = textView

        let ruler = CodeLineNumberRulerView(scrollView: scrollView, textView: textView)
        scrollView.verticalRulerView = ruler
        scrollView.hasVerticalRuler = true
        scrollView.rulersVisible = true
        context.coordinator.ruler = ruler

        apply(
            text: text,
            fileName: fileName,
            targetLine: targetLine,
            to: textView,
            coordinator: context.coordinator,
            preservingViewport: false
        )
        context.coordinator.lastFileName = fileName
        context.coordinator.lastTargetLine = targetLine
        context.coordinator.lastFontSize = fontSize
        schedulePosition(
            textView,
            in: scrollView,
            targetLine: targetLine,
            followsTail: followsTail,
            coordinator: context.coordinator
        )
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? NSTextView else { return }
        context.coordinator.parent = self
        textView.isEditable = isEditable
        textView.allowsUndo = isEditable
        (scrollView as? CodeScrollView)?.synchronizeDocumentFrame()
        let textChanged = textView.string != text
        let presentationChanged = context.coordinator.lastFileName != fileName
            || context.coordinator.lastTargetLine != targetLine
            || context.coordinator.lastFontSize != fontSize
        guard textChanged || presentationChanged else { return }

        apply(
            text: text,
            fileName: fileName,
            targetLine: targetLine,
            to: textView,
            coordinator: context.coordinator,
            preservingViewport: !presentationChanged && !followsTail
        )
        context.coordinator.lastFileName = fileName
        context.coordinator.lastTargetLine = targetLine
        context.coordinator.lastFontSize = fontSize
        if presentationChanged || followsTail {
            schedulePosition(
                textView,
                in: scrollView,
                targetLine: targetLine,
                followsTail: followsTail,
                coordinator: context.coordinator
            )
        }
    }

    private func configure(_ textView: NSTextView) {
        textView.isEditable = isEditable
        textView.isSelectable = true
        textView.isRichText = false
        textView.usesFindBar = true
        textView.isIncrementalSearchingEnabled = true
        textView.allowsUndo = isEditable
        textView.drawsBackground = true
        textView.backgroundColor = .textBackgroundColor
        textView.textContainerInset = NSSize(width: 16, height: 16)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = true
        textView.autoresizingMask = []
        textView.minSize = .zero
        textView.maxSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.textContainer?.widthTracksTextView = false
        textView.textContainer?.heightTracksTextView = false
        textView.textContainer?.containerSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false
        textView.isContinuousSpellCheckingEnabled = false
        textView.smartInsertDeleteEnabled = false
    }

    private func apply(
        text: String,
        fileName: String,
        targetLine: Int?,
        to textView: NSTextView,
        coordinator: Coordinator,
        preservingViewport: Bool
    ) {
        let selectedRanges = textView.selectedRanges
        let scrollView = textView.enclosingScrollView
        let viewportOrigin = scrollView?.contentView.bounds.origin
        let attributed = CodeSyntaxHighlighter.highlight(text, fileName: fileName, fontSize: fontSize)
        if let targetLine, let range = lineRange(targetLine, in: attributed.string) {
            attributed.addAttribute(
                .backgroundColor,
                value: NSColor.selectedContentBackgroundColor.withAlphaComponent(0.22),
                range: range
            )
        }

        coordinator.isApplyingText = true
        textView.textStorage?.setAttributedString(attributed)
        (textView.enclosingScrollView as? CodeScrollView)?.synchronizeDocumentFrame()
        if isEditable {
            let validRanges = selectedRanges.compactMap { value -> NSValue? in
                let range = value.rangeValue
                guard range.location <= attributed.length else { return nil }
                return NSValue(range: NSRange(
                    location: range.location,
                    length: min(range.length, attributed.length - range.location)
                ))
            }
            if !validRanges.isEmpty { textView.selectedRanges = validRanges }
        }
        coordinator.isApplyingText = false
        coordinator.ruler?.update(text: attributed.string, fontSize: fontSize)
        (textView.enclosingScrollView as? CodeScrollView)?.synchronizeDocumentFrame()
        if preservingViewport, let viewportOrigin, let scrollView {
            scrollView.contentView.scroll(to: viewportOrigin)
            scrollView.reflectScrolledClipView(scrollView.contentView)
        }
    }

    private func schedulePosition(
        _ textView: NSTextView,
        in scrollView: NSScrollView,
        targetLine: Int?,
        followsTail: Bool,
        coordinator: Coordinator
    ) {
        coordinator.positionGeneration += 1
        let generation = coordinator.positionGeneration

        position(
            textView,
            in: scrollView,
            targetLine: targetLine,
            followsTail: followsTail
        )

        // SwiftUI may resize the NSScrollView after updateNSView returns. Reset
        // the viewport again after AppKit has tiled the ruler and clip view, so
        // a previous file's horizontal offset cannot hide the first characters.
        DispatchQueue.main.async { [weak textView, weak scrollView, weak coordinator] in
            guard let textView,
                  let scrollView,
                  let coordinator,
                  coordinator.positionGeneration == generation else { return }
            scrollView.needsLayout = true
            scrollView.layoutSubtreeIfNeeded()
            scrollView.tile()
            position(
                textView,
                in: scrollView,
                targetLine: targetLine,
                followsTail: followsTail
            )
        }
    }

    private func position(
        _ textView: NSTextView,
        in scrollView: NSScrollView,
        targetLine: Int?,
        followsTail: Bool
    ) {
        let leadingX = -(scrollView.verticalRulerView?.requiredThickness ?? 0)
        if let targetLine,
           !textView.string.isEmpty,
           let characterLocation = lineStart(targetLine, in: textView.string),
           let layoutManager = textView.layoutManager {
            let glyphCharacterLocation = min(characterLocation, max(0, textView.string.utf16.count - 1))
            let glyphIndex = layoutManager.glyphIndexForCharacter(at: glyphCharacterLocation)
            var lineRect = layoutManager.lineFragmentRect(forGlyphAt: glyphIndex, effectiveRange: nil)
            lineRect.origin.y += textView.textContainerInset.height
            let y = max(0, lineRect.midY - scrollView.contentView.bounds.height / 2)
            scrollView.contentView.scroll(to: NSPoint(x: leadingX, y: y))
            scrollView.reflectScrolledClipView(scrollView.contentView)
        } else if followsTail {
            textView.scrollToEndOfDocument(nil)
            let maximumY = max(
                0,
                textView.bounds.height - scrollView.contentView.bounds.height
            )
            scrollView.contentView.scroll(to: NSPoint(x: leadingX, y: maximumY))
            scrollView.reflectScrolledClipView(scrollView.contentView)
        } else {
            scrollView.contentView.scroll(to: NSPoint(x: leadingX, y: 0))
            scrollView.reflectScrolledClipView(scrollView.contentView)
        }
    }

    private func lineStart(_ line: Int, in text: String) -> Int? {
        guard line > 0 else { return nil }
        let value = text as NSString
        var currentLine = 1
        var location = 0
        while currentLine < line, location < value.length {
            location = NSMaxRange(value.lineRange(for: NSRange(location: location, length: 0)))
            currentLine += 1
        }
        return currentLine == line ? min(location, value.length) : nil
    }

    private func lineRange(_ line: Int, in text: String) -> NSRange? {
        guard let start = lineStart(line, in: text) else { return nil }
        return (text as NSString).lineRange(for: NSRange(location: start, length: 0))
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: NativeCodeTextView
        weak var ruler: CodeLineNumberRulerView?
        var isApplyingText = false
        var lastFileName = ""
        var lastTargetLine: Int?
        var lastFontSize: CGFloat = 0
        var positionGeneration = 0

        init(parent: NativeCodeTextView) { self.parent = parent }

        func textDidChange(_ notification: Notification) {
            guard !isApplyingText,
                  parent.isEditable,
                  let textView = notification.object as? NSTextView else { return }
            parent.text = textView.string
            parent.apply(
                text: textView.string,
                fileName: parent.fileName,
                targetLine: parent.targetLine,
                to: textView,
                coordinator: self,
                preservingViewport: true
            )
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            guard !isApplyingText,
                  let textView = notification.object as? NSTextView else { return }
            parent.onSymbolSelection(Self.symbolSelection(in: textView))
        }

        @MainActor
        private static func symbolSelection(in textView: NSTextView) -> ProjectCodeSymbolSelection? {
            let value = textView.string as NSString
            guard value.length > 0 else { return nil }
            let selected = textView.selectedRange()
            var location = min(selected.location, value.length - 1)
            if !isIdentifierCodeUnit(value.character(at: location)),
               location > 0,
               isIdentifierCodeUnit(value.character(at: location - 1)) {
                location -= 1
            }
            guard isIdentifierCodeUnit(value.character(at: location)) else { return nil }

            var start = location
            var end = location + 1
            while start > 0, isIdentifierCodeUnit(value.character(at: start - 1)) { start -= 1 }
            while end < value.length, isIdentifierCodeUnit(value.character(at: end)) { end += 1 }
            let token = value.substring(with: NSRange(location: start, length: end - start))
            let prefix = value.substring(to: start)
            let line = prefix.reduce(into: 1) { count, character in
                if character == "\n" { count += 1 }
            }
            let lineStart = (prefix as NSString).range(of: "\n", options: .backwards).location
            let columnStart = lineStart == NSNotFound ? 0 : lineStart + 1
            let column = (prefix as NSString).substring(from: columnStart).count + 1
            return .init(token: token, line: line, column: column)
        }

        private static func isIdentifierCodeUnit(_ value: unichar) -> Bool {
            guard let scalar = UnicodeScalar(value) else { return false }
            return CharacterSet.alphanumerics.contains(scalar) || value == 95 || value == 36
        }
    }
}

private final class CodeScrollView: NSScrollView {
    private var isSynchronizingDocumentFrame = false

    override func layout() {
        super.layout()
        synchronizeDocumentFrame()
    }

    func synchronizeDocumentFrame() {
        guard !isSynchronizingDocumentFrame,
              let textView = documentView as? NSTextView,
              let layoutManager = textView.layoutManager,
              let textContainer = textView.textContainer else { return }
        isSynchronizingDocumentFrame = true
        defer { isSynchronizingDocumentFrame = false }

        layoutManager.ensureLayout(for: textContainer)
        let usedRect = layoutManager.usedRect(for: textContainer)
        let inset = textView.textContainerInset
        let viewport = contentView.bounds.size
        let size = NSSize(
            width: max(viewport.width, ceil(usedRect.maxX + inset.width * 2)),
            height: max(viewport.height, ceil(usedRect.maxY + inset.height * 2))
        )
        if abs(textView.frame.width - size.width) > 0.5
            || abs(textView.frame.height - size.height) > 0.5 {
            textView.setFrameSize(size)
        }
        if textView.frame.origin != .zero {
            textView.setFrameOrigin(.zero)
        }
    }
}

private final class CodeLineNumberRulerView: NSRulerView {
    private weak var textView: NSTextView?
    private var lineStarts: [Int] = [0]
    private var thickness: CGFloat = 44
    private var fontSize: CGFloat = 11

    init(scrollView: NSScrollView, textView: NSTextView) {
        self.textView = textView
        super.init(scrollView: scrollView, orientation: .verticalRuler)
        clientView = textView
    }

    required init(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override var requiredThickness: CGFloat { thickness }

    func update(text: String, fontSize: CGFloat) {
        let value = text as NSString
        var starts = [0]
        if value.length > 0 {
            for index in 0..<value.length where value.character(at: index) == 10 {
                starts.append(index + 1)
            }
        }
        lineStarts = starts
        self.fontSize = max(10, fontSize - 2)
        thickness = max(44, CGFloat(max(2, String(starts.count).count)) * 8 + 20)
        scrollView?.needsLayout = true
        scrollView?.tile()
        invalidateHashMarks()
        needsDisplay = true
    }

    override func drawHashMarksAndLabels(in rect: NSRect) {
        guard let textView,
              let layoutManager = textView.layoutManager,
              let textContainer = textView.textContainer else { return }
        NSColor.controlBackgroundColor.setFill()
        bounds.fill()
        NSColor.separatorColor.setFill()
        NSRect(x: bounds.maxX - 1, y: bounds.minY, width: 1, height: bounds.height).fill()

        let glyphRange = layoutManager.glyphRange(forBoundingRect: textView.visibleRect, in: textContainer)
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .right
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedSystemFont(ofSize: fontSize, weight: .regular),
            .foregroundColor: NSColor.tertiaryLabelColor,
            .paragraphStyle: paragraph,
        ]
        layoutManager.enumerateLineFragments(forGlyphRange: glyphRange) {
            [weak self] _, usedRect, _, lineGlyphRange, _ in
            guard let self else { return }
            let characterIndex = layoutManager.characterIndexForGlyph(at: lineGlyphRange.location)
            var textRect = usedRect
            textRect.origin.y += textView.textContainerInset.height
            let converted = textView.convert(textRect, to: self)
            NSString(string: "\(self.lineNumber(containing: characterIndex))").draw(
                in: NSRect(
                    x: 4,
                    y: converted.minY,
                    width: self.requiredThickness - 12,
                    height: max(16, converted.height)
                ),
                withAttributes: attributes
            )
        }
    }

    private func lineNumber(containing characterIndex: Int) -> Int {
        var lower = 0
        var upper = lineStarts.count
        while lower < upper {
            let middle = (lower + upper) / 2
            if lineStarts[middle] <= characterIndex { lower = middle + 1 }
            else { upper = middle }
        }
        return max(1, lower)
    }
}

private enum CodeSyntaxHighlighter {
    static func highlight(_ content: String, fileName: String, fontSize: CGFloat) -> NSMutableAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = 2
        paragraph.tabStops = []
        paragraph.defaultTabInterval = 32
        let result = NSMutableAttributedString(
            string: content,
            attributes: [
                .font: NSFont.monospacedSystemFont(ofSize: fontSize, weight: .regular),
                .foregroundColor: NSColor.labelColor,
                .paragraphStyle: paragraph,
            ]
        )
        guard !content.isEmpty, content.utf16.count <= 600_000 else { return result }
        let language = language(for: fileName)
        apply(pattern: numberPattern, color: .systemOrange, fontSize: fontSize, to: result)
        apply(pattern: keywordPattern(language), color: .systemPurple, fontSize: fontSize, weight: .semibold, to: result)
        apply(pattern: stringPattern(language), color: .systemRed, fontSize: fontSize, to: result)
        apply(pattern: commentPattern(language), color: .secondaryLabelColor, fontSize: fontSize, to: result)
        return result
    }

    private static func apply(
        pattern: String,
        color: NSColor,
        fontSize: CGFloat,
        weight: NSFont.Weight = .regular,
        to text: NSMutableAttributedString
    ) {
        guard let regex = try? NSRegularExpression(pattern: pattern, options: [.anchorsMatchLines]) else { return }
        for match in regex.matches(in: text.string, range: NSRange(location: 0, length: text.length)) {
            text.addAttribute(.foregroundColor, value: color, range: match.range)
            if weight != .regular {
                text.addAttribute(
                    .font,
                    value: NSFont.monospacedSystemFont(ofSize: fontSize, weight: weight),
                    range: match.range
                )
            }
        }
    }

    private static let numberPattern = #"(?<![A-Za-z_])(?:0x[0-9A-Fa-f]+|\d+(?:\.\d+)?)(?![A-Za-z_])"#

    private static func stringPattern(_ language: Language) -> String {
        switch language {
        case .shell, .python, .generic:
            #"\"(?:\\.|[^\"\\])*\"|'(?:\\.|[^'\\])*'"#
        case .javascript:
            #"\"(?:\\.|[^\"\\])*\"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`"#
        default:
            #"\"(?:\\.|[^\"\\])*\"|'(?:\\.|[^'\\])*'"#
        }
    }

    private static func commentPattern(_ language: Language) -> String {
        switch language {
        case .python, .shell, .yaml: #"#.*$"#
        case .json: #"(?!)"#
        default: #"//.*$|/\*[\s\S]*?\*/"#
        }
    }

    private static func keywordPattern(_ language: Language) -> String {
        let words: [String]
        switch language {
        case .swift:
            words = ["actor", "any", "as", "associatedtype", "async", "await", "break", "case", "catch", "class", "continue", "default", "defer", "do", "else", "enum", "extension", "false", "for", "func", "guard", "if", "import", "in", "init", "inout", "internal", "is", "let", "nil", "nonisolated", "open", "private", "protocol", "public", "repeat", "return", "self", "some", "static", "struct", "subscript", "super", "switch", "throw", "throws", "true", "try", "typealias", "var", "where", "while"]
        case .javascript:
            words = ["async", "await", "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do", "else", "export", "extends", "false", "finally", "for", "from", "function", "if", "import", "in", "instanceof", "let", "new", "null", "of", "return", "static", "super", "switch", "this", "throw", "true", "try", "typeof", "undefined", "var", "void", "while", "yield"]
        case .rust:
            words = ["as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while"]
        case .python:
            words = ["and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while", "with", "yield"]
        case .go:
            words = ["break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough", "for", "func", "go", "goto", "if", "import", "interface", "map", "package", "range", "return", "select", "struct", "switch", "type", "var"]
        case .java:
            words = ["abstract", "boolean", "break", "byte", "case", "catch", "char", "class", "const", "continue", "default", "do", "double", "else", "enum", "extends", "false", "final", "finally", "float", "for", "if", "implements", "import", "instanceof", "int", "interface", "long", "native", "new", "null", "package", "private", "protected", "public", "return", "short", "static", "strictfp", "super", "switch", "synchronized", "this", "throw", "throws", "transient", "true", "try", "void", "volatile", "while"]
        case .shell:
            words = ["case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if", "in", "local", "readonly", "return", "then", "until", "while"]
        case .json, .yaml, .generic:
            words = ["false", "null", "true"]
        }
        return #"\b(?:"# + words.map(NSRegularExpression.escapedPattern(for:)).joined(separator: "|") + #")\b"#
    }

    private static func language(for fileName: String) -> Language {
        switch URL(fileURLWithPath: fileName).pathExtension.lowercased() {
        case "swift": .swift
        case "js", "jsx", "mjs", "cjs", "ts", "tsx": .javascript
        case "rs": .rust
        case "py": .python
        case "go": .go
        case "java", "kt", "kts": .java
        case "sh", "bash", "zsh", "fish": .shell
        case "json", "jsonc": .json
        case "yaml", "yml", "toml": .yaml
        default: .generic
        }
    }

    private enum Language { case swift, javascript, rust, python, go, java, shell, json, yaml, generic }
}

private enum CodeFontMetrics {
    static func fontSize(for interfaceScale: CGFloat) -> CGFloat {
        max(11, 13 * interfaceScale)
    }
}
