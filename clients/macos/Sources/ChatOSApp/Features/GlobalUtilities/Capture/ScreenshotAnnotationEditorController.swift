import AppKit
import CoreGraphics

@MainActor
final class ScreenshotAnnotationEditorController: NSWindowController, NSWindowDelegate {
    var onComplete: ((CGImage) -> Void)?
    var onCancel: (() -> Void)?

    private let annotationView: ScreenshotAnnotationView
    private let toolControl: NSSegmentedControl
    private let undoButton: NSButton
    private let clearButton: NSButton
    private let isEnglish: Bool
    private var hasFinished = false

    init(image: CGImage, screen: NSScreen, isEnglish: Bool) {
        self.isEnglish = isEnglish
        self.annotationView = ScreenshotAnnotationView(image: image)
        self.toolControl = NSSegmentedControl(
            labels: [
                isEnglish ? "Pen" : "画笔",
                isEnglish ? "Rectangle" : "矩形",
            ],
            trackingMode: .selectOne,
            target: nil,
            action: nil
        )
        self.undoButton = NSButton(
            title: isEnglish ? "Undo" : "撤销",
            target: nil,
            action: nil
        )
        self.clearButton = NSButton(
            title: isEnglish ? "Clear" : "清空",
            target: nil,
            action: nil
        )

        let visible = screen.visibleFrame
        let width = min(max(760, visible.width * 0.78), 1_320)
        let height = min(max(560, visible.height * 0.82), 920)
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: NSSize(width: width, height: height)),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = isEnglish ? "Screenshot Annotation" : "截图标注"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.minSize = NSSize(width: 680, height: 480)
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        super.init(window: window)
        window.delegate = self
        configureContent()
        window.setFrameOrigin(NSPoint(
            x: visible.midX - width / 2,
            y: visible.midY - height / 2
        ))
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func present() {
        NSApp.activate(ignoringOtherApps: true)
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
    }

    func cancel() {
        finish(cancelled: true)
    }

    func windowWillClose(_ notification: Notification) {
        guard !hasFinished else { return }
        finish(cancelled: true)
    }

    private func configureContent() {
        guard let window else { return }

        let root = NSView()
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor(calibratedWhite: 0.075, alpha: 1).cgColor
        window.contentView = root

        let toolbar = NSVisualEffectView()
        toolbar.material = .headerView
        toolbar.blendingMode = .withinWindow
        toolbar.state = .active
        toolbar.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(toolbar)

        annotationView.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(annotationView)

        toolControl.selectedSegment = 0
        toolControl.target = self
        toolControl.action = #selector(toolChanged)

        let colorWell = NSColorWell()
        colorWell.color = .systemRed
        colorWell.colorWellStyle = .minimal
        colorWell.target = self
        colorWell.action = #selector(colorChanged(_:))

        undoButton.target = self
        undoButton.action = #selector(undo)
        undoButton.bezelStyle = .rounded
        undoButton.isEnabled = false

        clearButton.target = self
        clearButton.action = #selector(clear)
        clearButton.bezelStyle = .rounded
        clearButton.isEnabled = false

        let instruction = NSTextField(labelWithString: isEnglish
            ? "Drag on the image to annotate. The result is saved and copied when finished."
            : "在图片上拖动即可标注，完成后会保存并复制到剪贴板。")
        instruction.textColor = .secondaryLabelColor
        instruction.font = .systemFont(ofSize: 12)

        let cancelButton = NSButton(
            title: isEnglish ? "Cancel" : "取消",
            target: self,
            action: #selector(cancelPressed)
        )
        cancelButton.keyEquivalent = "\u{1b}"

        let doneButton = NSButton(
            title: isEnglish ? "Save & Copy" : "保存并复制",
            target: self,
            action: #selector(donePressed)
        )
        doneButton.bezelStyle = .rounded
        doneButton.keyEquivalent = "\r"

        let leadingStack = NSStackView(views: [toolControl, colorWell, undoButton, clearButton])
        leadingStack.orientation = .horizontal
        leadingStack.alignment = .centerY
        leadingStack.spacing = 8

        let trailingStack = NSStackView(views: [cancelButton, doneButton])
        trailingStack.orientation = .horizontal
        trailingStack.alignment = .centerY
        trailingStack.spacing = 8

        for stack in [leadingStack, trailingStack] {
            stack.translatesAutoresizingMaskIntoConstraints = false
            toolbar.addSubview(stack)
        }
        instruction.translatesAutoresizingMaskIntoConstraints = false
        toolbar.addSubview(instruction)

        NSLayoutConstraint.activate([
            toolbar.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            toolbar.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            toolbar.topAnchor.constraint(equalTo: root.topAnchor),
            toolbar.heightAnchor.constraint(equalToConstant: 72),

            annotationView.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            annotationView.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            annotationView.topAnchor.constraint(equalTo: toolbar.bottomAnchor),
            annotationView.bottomAnchor.constraint(equalTo: root.bottomAnchor),

            leadingStack.leadingAnchor.constraint(equalTo: toolbar.leadingAnchor, constant: 18),
            leadingStack.topAnchor.constraint(equalTo: toolbar.topAnchor, constant: 14),

            trailingStack.trailingAnchor.constraint(equalTo: toolbar.trailingAnchor, constant: -18),
            trailingStack.centerYAnchor.constraint(equalTo: leadingStack.centerYAnchor),

            instruction.leadingAnchor.constraint(equalTo: leadingStack.leadingAnchor),
            instruction.topAnchor.constraint(equalTo: leadingStack.bottomAnchor, constant: 7),
            instruction.trailingAnchor.constraint(lessThanOrEqualTo: trailingStack.leadingAnchor, constant: -16),
        ])

        annotationView.onAnnotationsChanged = { [weak self] in
            guard let self else { return }
            self.undoButton.isEnabled = self.annotationView.canUndo
            self.clearButton.isEnabled = self.annotationView.hasAnnotations
        }
    }

    @objc private func toolChanged() {
        annotationView.tool = ScreenshotAnnotationTool(rawValue: toolControl.selectedSegment) ?? .pen
        annotationView.window?.invalidateCursorRects(for: annotationView)
    }

    @objc private func colorChanged(_ sender: NSColorWell) {
        annotationView.annotationColor = sender.color
    }

    @objc private func undo() {
        annotationView.undo()
    }

    @objc private func clear() {
        annotationView.clear()
    }

    @objc private func cancelPressed() {
        finish(cancelled: true)
    }

    @objc private func donePressed() {
        guard !hasFinished, let image = annotationView.renderedImage() else { return }
        hasFinished = true
        window?.orderOut(nil)
        onComplete?(image)
        onComplete = nil
        onCancel = nil
    }

    private func finish(cancelled: Bool) {
        guard !hasFinished else { return }
        hasFinished = true
        window?.orderOut(nil)
        if cancelled {
            onCancel?()
        }
        onComplete = nil
        onCancel = nil
    }
}
