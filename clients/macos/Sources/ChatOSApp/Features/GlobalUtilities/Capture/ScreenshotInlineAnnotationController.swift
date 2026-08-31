import AppKit
import CoreGraphics

@MainActor
final class ScreenshotInlineAnnotationController {
    var onComplete: ((CGImage) -> Void)?
    var onCancel: (() -> Void)?
    var onRequestLongCapture: (() -> Void)?

    private let annotationView: ScreenshotAnnotationView
    private let selection: ScreenSelection
    private let isEnglish: Bool
    private var backdropWindows: [NSPanel] = []
    private var canvasWindow: NSPanel?
    private var toolbarWindow: NSPanel?
    private var keyMonitor: Any?
    private var hasFinished = false
    private weak var undoButton: NSButton?
    private weak var clearButton: NSButton?
    private weak var longCaptureButton: NSButton?

    init(image: CGImage, selection: ScreenSelection, isEnglish: Bool) {
        annotationView = ScreenshotAnnotationView(image: image)
        self.selection = selection
        self.isEnglish = isEnglish
    }

    func present() {
        guard canvasWindow == nil, toolbarWindow == nil else { return }
        presentBackdrops()
        presentCanvas()
        presentToolbar()
        installKeyMonitor()
    }

    func cancel() {
        finish(cancelled: true)
    }

    private func presentBackdrops() {
        for screen in NSScreen.screens {
            let panel = ScreenshotOverlayPanel(
                contentRect: screen.frame,
                styleMask: [.borderless, .nonactivatingPanel],
                backing: .buffered,
                defer: false,
                screen: screen
            )
            let view = ScreenshotBackdropView(frame: NSRect(origin: .zero, size: screen.frame.size))
            view.autoresizingMask = [.width, .height]
            view.onCancel = { [weak self] in self?.cancel() }
            panel.contentView = view
            configureOverlayPanel(panel, levelOffset: 0)
            panel.orderFrontRegardless()
            backdropWindows.append(panel)
        }
    }

    private func presentCanvas() {
        let panel = ScreenshotOverlayPanel(
            contentRect: selection.globalRect,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false,
            screen: selection.screen
        )
        annotationView.frame = NSRect(origin: .zero, size: selection.globalRect.size)
        annotationView.autoresizingMask = [.width, .height]
        annotationView.onAnnotationsChanged = { [weak self] in
            guard let self else { return }
            self.undoButton?.isEnabled = self.annotationView.canUndo
            self.clearButton?.isEnabled = self.annotationView.hasAnnotations
            self.longCaptureButton?.isEnabled = !self.annotationView.hasAnnotations
        }
        panel.contentView = annotationView
        configureOverlayPanel(panel, levelOffset: 1)
        panel.hasShadow = true
        panel.onCancel = { [weak self] in self?.cancel() }
        panel.orderFrontRegardless()
        canvasWindow = panel
    }

    private func presentToolbar() {
        let toolbarSize = NSSize(width: 570, height: 48)
        let panel = ScreenshotOverlayPanel(
            contentRect: NSRect(origin: toolbarOrigin(for: toolbarSize), size: toolbarSize),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false,
            screen: selection.screen
        )
        configureOverlayPanel(panel, levelOffset: 2)
        panel.hasShadow = true
        panel.contentView = makeToolbarView(size: toolbarSize)
        panel.onCancel = { [weak self] in self?.cancel() }
        panel.orderFrontRegardless()
        panel.makeKey()
        toolbarWindow = panel
    }

    private func makeToolbarView(size: NSSize) -> NSView {
        let material = NSVisualEffectView(frame: NSRect(origin: .zero, size: size))
        material.material = .hudWindow
        material.blendingMode = .behindWindow
        material.state = .active
        material.wantsLayer = true
        material.layer?.cornerRadius = 12
        material.layer?.masksToBounds = true

        let tools = NSSegmentedControl(
            labels: ["", "", "", "", ""],
            trackingMode: .selectOne,
            target: self,
            action: #selector(toolChanged(_:))
        )
        tools.selectedSegment = 0
        tools.setImage(symbol("pencil.tip", description: localized("画笔", "Pen")), forSegment: 0)
        tools.setImage(symbol("rectangle", description: localized("矩形", "Rectangle")), forSegment: 1)
        tools.setImage(symbol("circle", description: localized("圆形或椭圆", "Circle or ellipse")), forSegment: 2)
        tools.setImage(symbol("arrow.up.right", description: localized("箭头", "Arrow")), forSegment: 3)
        tools.setImage(symbol("textformat", description: localized("文字", "Text")), forSegment: 4)
        let toolTips = [
            localized("画笔", "Pen"),
            localized("矩形", "Rectangle"),
            localized("圆形或椭圆", "Circle or ellipse"),
            localized("箭头", "Arrow"),
            localized("文字", "Text"),
        ]
        for index in 0..<5 {
            tools.setWidth(38, forSegment: index)
            tools.setToolTip(toolTips[index], forSegment: index)
        }

        let colorWell = NSColorWell()
        colorWell.color = .systemRed
        colorWell.colorWellStyle = .minimal
        colorWell.target = self
        colorWell.action = #selector(colorChanged(_:))
        colorWell.toolTip = localized("标注颜色", "Annotation color")

        let longCapture = iconButton(
            "arrow.down.to.line",
            help: localized("长截图", "Long screenshot"),
            action: #selector(longCapturePressed)
        )
        longCaptureButton = longCapture

        let undo = iconButton(
            "arrow.uturn.backward",
            help: localized("撤销", "Undo"),
            action: #selector(undoAnnotation)
        )
        undo.isEnabled = false
        undoButton = undo

        let clear = iconButton(
            "trash",
            help: localized("清空标注", "Clear annotations"),
            action: #selector(clearAnnotations)
        )
        clear.isEnabled = false
        clearButton = clear

        let cancel = iconButton(
            "xmark",
            help: localized("取消", "Cancel"),
            action: #selector(cancelPressed)
        )
        cancel.keyEquivalent = "\u{1b}"

        let done = iconButton(
            "checkmark",
            help: localized("完成、保存并复制", "Finish, save and copy"),
            action: #selector(donePressed)
        )
        done.contentTintColor = .systemGreen
        done.keyEquivalent = "\r"

        let divider = NSBox()
        divider.boxType = .separator
        divider.translatesAutoresizingMaskIntoConstraints = false
        divider.setContentHuggingPriority(.required, for: .horizontal)

        let stack = NSStackView(views: [
            tools,
            colorWell,
            longCapture,
            undo,
            clear,
            divider,
            cancel,
            done,
        ])
        stack.orientation = .horizontal
        stack.alignment = .centerY
        stack.spacing = 9
        stack.translatesAutoresizingMaskIntoConstraints = false
        material.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: material.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: material.centerYAnchor),
            divider.heightAnchor.constraint(equalToConstant: 24),
        ])
        return material
    }

    private func installKeyMonitor() {
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }
            if self.annotationView.isEditingText {
                return event
            }
            if event.keyCode == 53 {
                self.cancel()
                return nil
            }
            if event.keyCode == 36 || event.keyCode == 76 {
                self.complete()
                return nil
            }
            if event.modifierFlags.contains(.command), event.charactersIgnoringModifiers == "z" {
                self.annotationView.undo()
                return nil
            }
            return event
        }
    }

    private func configureOverlayPanel(_ panel: NSPanel, levelOffset: Int) {
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.isReleasedWhenClosed = false
        panel.level = NSWindow.Level(rawValue: NSWindow.Level.screenSaver.rawValue + levelOffset)
        panel.collectionBehavior = [
            .canJoinAllSpaces,
            .fullScreenAuxiliary,
            .ignoresCycle,
        ]
        panel.animationBehavior = .none
        panel.isFloatingPanel = true
        panel.hidesOnDeactivate = false
        panel.becomesKeyOnlyIfNeeded = false
    }

    private func toolbarOrigin(for size: NSSize) -> NSPoint {
        let visible = selection.screen.visibleFrame
        let horizontal = min(
            max(selection.globalRect.midX - size.width / 2, visible.minX + 8),
            visible.maxX - size.width - 8
        )
        let below = selection.globalRect.minY - size.height - 10
        if below >= visible.minY + 8 {
            return NSPoint(x: horizontal, y: below)
        }
        let above = selection.globalRect.maxY + 10
        return NSPoint(
            x: horizontal,
            y: min(above, visible.maxY - size.height - 8)
        )
    }

    @objc private func toolChanged(_ sender: NSSegmentedControl) {
        annotationView.tool = ScreenshotAnnotationTool(rawValue: sender.selectedSegment) ?? .pen
        annotationView.window?.invalidateCursorRects(for: annotationView)
    }

    @objc private func colorChanged(_ sender: NSColorWell) {
        annotationView.annotationColor = sender.color
    }

    @objc private func undoAnnotation() {
        annotationView.undo()
    }

    @objc private func clearAnnotations() {
        annotationView.clear()
    }

    @objc private func cancelPressed() {
        cancel()
    }

    @objc private func donePressed() {
        complete()
    }

    @objc private func longCapturePressed() {
        guard !hasFinished, !annotationView.hasAnnotations else { return }
        hasFinished = true
        dismissWindows()
        onRequestLongCapture?()
        onComplete = nil
        onCancel = nil
        onRequestLongCapture = nil
    }

    private func complete() {
        guard !hasFinished, let image = annotationView.renderedImage() else { return }
        hasFinished = true
        dismissWindows()
        onComplete?(image)
        onComplete = nil
        onCancel = nil
        onRequestLongCapture = nil
    }

    private func finish(cancelled: Bool) {
        guard !hasFinished else { return }
        hasFinished = true
        dismissWindows()
        if cancelled {
            onCancel?()
        }
        onComplete = nil
        onCancel = nil
        onRequestLongCapture = nil
    }

    private func dismissWindows() {
        if let keyMonitor {
            NSEvent.removeMonitor(keyMonitor)
            self.keyMonitor = nil
        }
        toolbarWindow?.orderOut(nil)
        canvasWindow?.orderOut(nil)
        backdropWindows.forEach { $0.orderOut(nil) }
        toolbarWindow = nil
        canvasWindow = nil
        backdropWindows.removeAll()
    }

    private func iconButton(_ symbolName: String, help: String, action: Selector) -> NSButton {
        let button = NSButton(title: "", target: self, action: action)
        button.image = symbol(symbolName, description: help)
        button.imagePosition = .imageOnly
        button.imageScaling = .scaleProportionallyDown
        button.bezelStyle = .circular
        button.toolTip = help
        return button
    }

    private func symbol(_ name: String, description: String) -> NSImage {
        NSImage(systemSymbolName: name, accessibilityDescription: description)
            ?? NSImage(size: NSSize(width: 16, height: 16))
    }

    private func localized(_ chinese: String, _ english: String) -> String {
        isEnglish ? english : chinese
    }
}

private final class ScreenshotOverlayPanel: NSPanel {
    var onCancel: (() -> Void)?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }
}

private final class ScreenshotBackdropView: NSView {
    var onCancel: (() -> Void)?

    override func draw(_ dirtyRect: NSRect) {
        NSColor.black.withAlphaComponent(0.38).setFill()
        bounds.fill()
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func mouseDown(with event: NSEvent) {
        onCancel?()
    }
}
