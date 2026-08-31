import AppKit
import SwiftUI

enum ComposerPasteContent {
    case files([URL])
    case image(data: Data, mimeType: String, suggestedName: String)
    case document(data: Data, mimeType: String, suggestedName: String)
    case longText(String)
}

struct ComposerPasteTextEditor: View {
    private enum Layout {
        static let minimumHeight: CGFloat = 34
        static let maximumHeight: CGFloat = 126
    }

    @Binding var text: String
    var placeholder: String
    var onSubmit: () -> Void
    var onPasteContent: (ComposerPasteContent) -> Void

    @Environment(\.interfaceFontScale) private var interfaceFontScale
    @State private var measuredHeight = Layout.minimumHeight

    var body: some View {
        NativeComposerTextEditor(
            text: $text,
            measuredHeight: $measuredHeight,
            placeholder: placeholder,
            fontSize: max(8, 14 * interfaceFontScale),
            onSubmit: onSubmit,
            onPasteContent: onPasteContent
        )
        .frame(
            minHeight: Layout.minimumHeight,
            idealHeight: measuredHeight,
            maxHeight: Layout.maximumHeight
        )
    }
}

private struct NativeComposerTextEditor: NSViewRepresentable {
    @Binding var text: String
    @Binding var measuredHeight: CGFloat
    let placeholder: String
    let fontSize: CGFloat
    let onSubmit: () -> Void
    let onPasteContent: (ComposerPasteContent) -> Void

    private let minimumHeight: CGFloat = 34
    private let maximumHeight: CGFloat = 126

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.borderType = .noBorder
        scrollView.hasHorizontalScroller = false
        scrollView.hasVerticalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.contentInsets = NSEdgeInsets()

        let textView = ComposerNativeTextView()
        textView.delegate = context.coordinator
        textView.isRichText = false
        textView.importsGraphics = false
        textView.allowsUndo = true
        textView.usesFindBar = true
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.textColor = .labelColor
        textView.insertionPointColor = .controlAccentColor
        textView.textContainerInset = NSSize(width: 0, height: 7)
        textView.textContainer?.lineFragmentPadding = 0
        textView.isHorizontallyResizable = false
        textView.isVerticallyResizable = true
        textView.autoresizingMask = [.width]
        textView.minSize = NSSize(width: 0, height: minimumHeight)
        textView.maxSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.textContainer?.widthTracksTextView = true
        textView.textContainer?.heightTracksTextView = false
        textView.textContainer?.containerSize = NSSize(
            width: 0,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.string = text
        textView.placeholder = placeholder
        configureTransparentBackground(scrollView: scrollView, textView: textView)
        configure(textView, coordinator: context.coordinator)

        scrollView.documentView = textView
        context.coordinator.textView = textView
        context.coordinator.scrollView = scrollView
        context.coordinator.scheduleHeightMeasurement()
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? ComposerNativeTextView else { return }
        context.coordinator.parent = self
        textView.placeholder = placeholder
        configureTransparentBackground(scrollView: scrollView, textView: textView)
        configure(textView, coordinator: context.coordinator)

        if textView.string != text {
            let selection = textView.selectedRange()
            textView.string = text
            textView.setSelectedRange(
                NSRange(
                    location: min(selection.location, text.utf16.count),
                    length: 0
                )
            )
            textView.needsDisplay = true
        }
        context.coordinator.scheduleHeightMeasurement()
    }

    private func configureTransparentBackground(
        scrollView: NSScrollView,
        textView: ComposerNativeTextView
    ) {
        // AppKit can restore the default text/clip-view background when the
        // effective appearance changes. Keep every layer transparent so the
        // SwiftUI input surface remains visible in both light and dark mode.
        scrollView.drawsBackground = false
        scrollView.backgroundColor = .clear
        scrollView.contentView.drawsBackground = false
        scrollView.contentView.backgroundColor = .clear
        textView.drawsBackground = false
        textView.backgroundColor = .clear
    }

    private func configure(_ textView: ComposerNativeTextView, coordinator: Coordinator) {
        let font = NSFont.systemFont(ofSize: fontSize)
        if textView.font != font {
            textView.font = font
            textView.typingAttributes[.font] = font
            textView.needsDisplay = true
        }
        textView.onSubmit = onSubmit
        textView.onPaste = { pasteboard in
            guard let content = ComposerPasteboardReader.content(from: pasteboard) else {
                return false
            }
            onPasteContent(content)
            return true
        }
        textView.onLayoutChanged = {
            coordinator.scheduleHeightMeasurement()
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: NativeComposerTextEditor
        weak var textView: ComposerNativeTextView?
        weak var scrollView: NSScrollView?
        private var measurementScheduled = false

        init(parent: NativeComposerTextEditor) {
            self.parent = parent
        }

        func textDidChange(_ notification: Notification) {
            guard let textView = notification.object as? NSTextView else { return }
            parent.text = textView.string
            scheduleHeightMeasurement()
        }

        func scheduleHeightMeasurement() {
            guard !measurementScheduled else { return }
            measurementScheduled = true
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                measurementScheduled = false
                measureHeight()
            }
        }

        private func measureHeight() {
            guard let textView,
                  let layoutManager = textView.layoutManager,
                  let textContainer = textView.textContainer else { return }
            let availableWidth = max(1, textView.bounds.width)
            if textContainer.containerSize.width != availableWidth {
                textContainer.containerSize.width = availableWidth
            }
            layoutManager.ensureLayout(for: textContainer)
            let usedHeight = layoutManager.usedRect(for: textContainer).height
            let verticalInsets = textView.textContainerInset.height * 2
            let desiredHeight = min(
                parent.maximumHeight,
                max(parent.minimumHeight, ceil(usedHeight + verticalInsets))
            )

            if abs(parent.measuredHeight - desiredHeight) > 0.5 {
                parent.measuredHeight = desiredHeight
            }
            scrollView?.hasVerticalScroller = desiredHeight >= parent.maximumHeight
            textView.needsDisplay = true
        }
    }
}

private final class ComposerNativeTextView: NSTextView {
    var placeholder = "" {
        didSet { needsDisplay = true }
    }
    var onSubmit: (() -> Void)?
    var onPaste: ((NSPasteboard) -> Bool)?
    var onLayoutChanged: (() -> Void)?

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let isCommandPaste = event.type == .keyDown
            && modifiers == .command
            && event.charactersIgnoringModifiers?.lowercased() == "v"
        if isCommandPaste, onPaste?(.general) == true {
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func keyDown(with event: NSEvent) {
        let isReturn = event.keyCode == 36 || event.keyCode == 76
        if isReturn,
           !event.modifierFlags.contains(.shift),
           !hasMarkedText() {
            onSubmit?()
            return
        }
        super.keyDown(with: event)
    }

    override func paste(_ sender: Any?) {
        if onPaste?(.general) == true { return }
        super.paste(sender)
    }

    override func didChangeText() {
        super.didChangeText()
        needsDisplay = true
        onLayoutChanged?()
    }

    override func setFrameSize(_ newSize: NSSize) {
        let widthChanged = abs(frame.width - newSize.width) > 0.5
        super.setFrameSize(newSize)
        if widthChanged {
            onLayoutChanged?()
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty, !placeholder.isEmpty else { return }

        let attributes: [NSAttributedString.Key: Any] = [
            .font: font ?? NSFont.systemFont(ofSize: 14),
            .foregroundColor: NSColor.placeholderTextColor,
        ]
        let linePadding = textContainer?.lineFragmentPadding ?? 0
        let origin = NSPoint(
            x: textContainerInset.width + linePadding,
            y: textContainerInset.height
        )
        placeholder.draw(at: origin, withAttributes: attributes)
    }
}

@MainActor
private enum ComposerPasteboardReader {
    static func content(from pasteboard: NSPasteboard) -> ComposerPasteContent? {
        let fileURLs = fileURLs(from: pasteboard)
        if !fileURLs.isEmpty {
            return .files(fileURLs)
        }

        let pngType = NSPasteboard.PasteboardType("public.png")
        if let data = pasteboard.data(forType: pngType), !data.isEmpty {
            return .image(
                data: data,
                mimeType: "image/png",
                suggestedName: timestampedName("粘贴的图片", extension: "png")
            )
        }

        let jpegType = NSPasteboard.PasteboardType("public.jpeg")
        if let data = pasteboard.data(forType: jpegType), !data.isEmpty {
            return .image(
                data: data,
                mimeType: "image/jpeg",
                suggestedName: timestampedName("粘贴的图片", extension: "jpg")
            )
        }

        if let tiff = pasteboard.data(forType: .tiff),
           let image = NSImage(data: tiff),
           let png = image.pngData {
            return .image(
                data: png,
                mimeType: "image/png",
                suggestedName: timestampedName("粘贴的图片", extension: "png")
            )
        }

        if let pdf = pasteboard.data(forType: .pdf), !pdf.isEmpty {
            return .document(
                data: pdf,
                mimeType: "application/pdf",
                suggestedName: timestampedName("粘贴的文档", extension: "pdf")
            )
        }

        guard let pastedText = pasteboard.string(forType: .string),
              pastedText.count >= ConversationSessionViewModel.longPasteCharacterThreshold
                || pastedText.utf8.count >= ConversationSessionViewModel.longPasteByteThreshold
        else { return nil }
        return .longText(pastedText)
    }

    private static func fileURLs(from pasteboard: NSPasteboard) -> [URL] {
        if let values = pasteboard.readObjects(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true]
        ) as? [URL], !values.isEmpty {
            return values
        }

        if let value = pasteboard.string(forType: .fileURL),
           let url = URL(string: value), url.isFileURL {
            return [url]
        }

        let legacyFilenamesType = NSPasteboard.PasteboardType("NSFilenamesPboardType")
        guard let paths = pasteboard.propertyList(forType: legacyFilenamesType) as? [String] else {
            return []
        }
        return paths.map { URL(fileURLWithPath: $0) }
    }

    private static func timestampedName(
        _ prefix: String,
        extension fileExtension: String
    ) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH.mm.ss"
        return "\(prefix) \(formatter.string(from: Date())).\(fileExtension)"
    }
}

private extension NSImage {
    var pngData: Data? {
        guard let tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiffRepresentation) else { return nil }
        return bitmap.representation(using: .png, properties: [:])
    }
}
