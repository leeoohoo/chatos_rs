import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct NotepadMarkdownEditor: NSViewRepresentable {
    @Binding var text: String
    var onPasteImage: (NotepadImageUpload, String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true

        let textView = NotepadMarkdownNativeTextView()
        textView.delegate = context.coordinator
        textView.isRichText = false
        textView.importsGraphics = false
        textView.allowsUndo = true
        textView.drawsBackground = false
        textView.font = .monospacedSystemFont(ofSize: 13, weight: .regular)
        textView.textColor = .labelColor
        textView.insertionPointColor = .labelColor
        textView.textContainerInset = NSSize(width: 3, height: 5)
        textView.isHorizontallyResizable = false
        textView.isVerticallyResizable = true
        textView.autoresizingMask = [.width]
        textView.minSize = .zero
        textView.maxSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.textContainer?.widthTracksTextView = true
        textView.textContainer?.containerSize = NSSize(
            width: 0,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.string = text
        textView.onPasteImage = { [weak coordinator = context.coordinator, weak textView] image, placeholder in
            guard let coordinator, let textView else { return }
            coordinator.parent.text = textView.string
            coordinator.parent.onPasteImage(image, placeholder)
        }
        scrollView.documentView = textView
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let textView = scrollView.documentView as? NotepadMarkdownNativeTextView else { return }
        textView.onPasteImage = { [weak coordinator = context.coordinator, weak textView] image, placeholder in
            guard let coordinator, let textView else { return }
            coordinator.parent.text = textView.string
            coordinator.parent.onPasteImage(image, placeholder)
        }
        guard !textView.hasMarkedText(), textView.string != text else { return }
        let selection = textView.selectedRange()
        textView.string = text
        let end = (text as NSString).length
        textView.setSelectedRange(NSRange(location: min(selection.location, end), length: 0))
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: NotepadMarkdownEditor

        init(parent: NotepadMarkdownEditor) {
            self.parent = parent
        }

        func textDidChange(_ notification: Notification) {
            guard let textView = notification.object as? NSTextView else { return }
            parent.text = textView.string
        }
    }
}

private final class NotepadMarkdownNativeTextView: NSTextView {
    var onPasteImage: ((NotepadImageUpload, String) -> Void)?

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if event.type == .keyDown,
           modifiers == .command,
           event.charactersIgnoringModifiers?.lowercased() == "v",
           pasteImage(from: .general) {
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func paste(_ sender: Any?) {
        if pasteImage(from: .general) { return }
        super.paste(sender)
    }

    private func pasteImage(from pasteboard: NSPasteboard) -> Bool {
        guard let image = NotepadImagePasteboardReader.image(from: pasteboard) else { return false }
        let placeholder = "![正在上传图片…](chatos-uploading://\(UUID().uuidString.lowercased()))"
        let prefix = selectedRange().location > 0 ? "\n\n" : ""
        insertText(prefix + placeholder + "\n\n", replacementRange: selectedRange())
        onPasteImage?(image, placeholder)
        return true
    }
}

@MainActor
private enum NotepadImagePasteboardReader {
    static func image(from pasteboard: NSPasteboard) -> NotepadImageUpload? {
        let pngType = NSPasteboard.PasteboardType("public.png")
        if let data = pasteboard.data(forType: pngType), !data.isEmpty {
            return upload(data: data, mimeType: "image/png", extension: "png")
        }

        let jpegType = NSPasteboard.PasteboardType("public.jpeg")
        if let data = pasteboard.data(forType: jpegType), !data.isEmpty {
            return upload(data: data, mimeType: "image/jpeg", extension: "jpg")
        }

        let webPType = NSPasteboard.PasteboardType("org.webmproject.webp")
        if let data = pasteboard.data(forType: webPType), !data.isEmpty {
            return upload(data: data, mimeType: "image/webp", extension: "webp")
        }

        if let tiff = pasteboard.data(forType: .tiff),
           let image = NSImage(data: tiff),
           let png = image.notepadPNGData {
            return upload(data: png, mimeType: "image/png", extension: "png")
        }

        guard let url = fileURL(from: pasteboard),
              let type = UTType(filenameExtension: url.pathExtension),
              type.conforms(to: .image),
              let data = try? Data(contentsOf: url, options: [.mappedIfSafe]),
              !data.isEmpty else { return nil }
        if type.conforms(to: .png) {
            return NotepadImageUpload(data: data, mimeType: "image/png", name: url.lastPathComponent)
        }
        if type.conforms(to: .jpeg) {
            return NotepadImageUpload(data: data, mimeType: "image/jpeg", name: url.lastPathComponent)
        }
        if type.identifier == UTType.webP.identifier {
            return NotepadImageUpload(data: data, mimeType: "image/webp", name: url.lastPathComponent)
        }
        guard let image = NSImage(data: data), let png = image.notepadPNGData else { return nil }
        return upload(data: png, mimeType: "image/png", extension: "png")
    }

    private static func upload(
        data: Data,
        mimeType: String,
        extension fileExtension: String
    ) -> NotepadImageUpload {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH.mm.ss"
        return NotepadImageUpload(
            data: data,
            mimeType: mimeType,
            name: "笔记图片 \(formatter.string(from: Date())).\(fileExtension)"
        )
    }

    private static func fileURL(from pasteboard: NSPasteboard) -> URL? {
        if let urls = pasteboard.readObjects(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true]
        ) as? [URL] {
            return urls.first
        }
        guard let value = pasteboard.string(forType: .fileURL),
              let url = URL(string: value), url.isFileURL else { return nil }
        return url
    }
}

private extension NSImage {
    var notepadPNGData: Data? {
        guard let tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiffRepresentation) else { return nil }
        return bitmap.representation(using: .png, properties: [:])
    }
}
