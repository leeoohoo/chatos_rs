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
    private var escapeHotKeyMonitor: ScreenshotEscapeHotKeyMonitor?
    private var hasFinished = false
    private weak var toolControl: NSSegmentedControl?
    private weak var undoButton: NSButton?
    private weak var redoButton: NSButton?
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
            let view = ScreenshotBackdropView(
                frame: NSRect(origin: .zero, size: screen.frame.size),
                clearRect: localClearRect(on: screen)
            )
            view.autoresizingMask = [.width, .height]
            view.onCancel = { [weak self] in self?.cancel() }
            panel.contentView = view
            configureOverlayPanel(panel, levelOffset: 0)
            panel.orderFrontRegardless()
            backdropWindows.append(panel)
        }
    }

    private func localClearRect(on screen: NSScreen) -> NSRect? {
        let intersection = selection.globalRect.intersection(screen.frame)
        guard !intersection.isNull, !intersection.isEmpty else { return nil }
        return NSRect(
            x: intersection.minX - screen.frame.minX,
            y: intersection.minY - screen.frame.minY,
            width: intersection.width,
            height: intersection.height
        )
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
            self.redoButton?.isEnabled = self.annotationView.canRedo
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
        let toolbarSize = NSSize(width: 780, height: 52)
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
            labels: Array(repeating: "", count: 9),
            trackingMode: .selectOne,
            target: self,
            action: #selector(toolChanged(_:))
        )
        tools.selectedSegment = 0
        tools.setImage(symbol("pencil.tip", description: localized("画笔", "Pen")), forSegment: 0)
        tools.setImage(symbol("line.diagonal", description: localized("直线", "Line")), forSegment: 1)
        tools.setImage(symbol("rectangle", description: localized("矩形", "Rectangle")), forSegment: 2)
        tools.setImage(symbol("circle", description: localized("圆形或椭圆", "Circle or ellipse")), forSegment: 3)
        tools.setImage(symbol("arrow.up.right", description: localized("箭头", "Arrow")), forSegment: 4)
        tools.setImage(symbol("highlighter", description: localized("高亮", "Highlight")), forSegment: 5)
        tools.setImage(symbol("square.grid.3x3", description: localized("马赛克", "Mosaic")), forSegment: 6)
        tools.setImage(symbol("textformat", description: localized("文字", "Text")), forSegment: 7)
        tools.setImage(symbol("1.circle", description: localized("序号", "Number")), forSegment: 8)
        let toolTips = [
            localized("画笔 (P / 1)", "Pen (P / 1)"),
            localized("直线 (L / 2)", "Line (L / 2)"),
            localized("矩形 (R / 3)", "Rectangle (R / 3)"),
            localized("椭圆 (O / 4)", "Ellipse (O / 4)"),
            localized("箭头 (A / 5)", "Arrow (A / 5)"),
            localized("高亮 (H / 6)", "Highlight (H / 6)"),
            localized("马赛克 (M / 7)", "Mosaic (M / 7)"),
            localized("文字 (T / 8)", "Text (T / 8)"),
            localized("序号 (N / 9)", "Number (N / 9)"),
        ]
        for index in 0..<9 {
            tools.setWidth(32, forSegment: index)
            tools.setToolTip(toolTips[index], forSegment: index)
        }
        toolControl = tools

        let colorWell = NSColorWell()
        colorWell.color = .systemRed
        colorWell.colorWellStyle = .minimal
        colorWell.target = self
        colorWell.action = #selector(colorChanged(_:))
        colorWell.toolTip = localized("标注颜色", "Annotation color")

        let lineWidth = NSSlider(
            value: 1,
            minValue: 0.5,
            maxValue: 3,
            target: self,
            action: #selector(lineWidthChanged(_:))
        )
        lineWidth.frame.size.width = 74
        lineWidth.toolTip = localized("线条粗细", "Line width")

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

        let redo = iconButton(
            "arrow.uturn.forward",
            help: localized("重做", "Redo"),
            action: #selector(redoAnnotation)
        )
        redo.isEnabled = false
        redoButton = redo

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
            lineWidth,
            longCapture,
            undo,
            redo,
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
            lineWidth.widthAnchor.constraint(equalToConstant: 74),
        ])
        return material
    }

    private func installKeyMonitor() {
        let escapeHotKeyMonitor = ScreenshotEscapeHotKeyMonitor { [weak self] in
            self?.cancel()
        }
        escapeHotKeyMonitor.start()
        self.escapeHotKeyMonitor = escapeHotKeyMonitor

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
                if event.modifierFlags.contains(.shift) {
                    self.annotationView.redo()
                } else {
                    self.annotationView.undo()
                }
                return nil
            }
            let blockedModifiers: NSEvent.ModifierFlags = [.command, .control, .option]
            if event.modifierFlags.intersection(blockedModifiers).isEmpty,
               let value = event.charactersIgnoringModifiers,
               let tool = ScreenshotAnnotationTool.shortcut(value) {
                self.selectTool(tool)
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
        selectTool(ScreenshotAnnotationTool(rawValue: sender.selectedSegment) ?? .pen)
    }

    @objc private func colorChanged(_ sender: NSColorWell) {
        annotationView.annotationColor = sender.color
    }

    @objc private func lineWidthChanged(_ sender: NSSlider) {
        annotationView.lineWidthScale = CGFloat(sender.doubleValue)
    }

    @objc private func undoAnnotation() {
        annotationView.undo()
    }

    @objc private func redoAnnotation() {
        annotationView.redo()
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
        escapeHotKeyMonitor?.stop()
        escapeHotKeyMonitor = nil
        toolbarWindow?.orderOut(nil)
        canvasWindow?.orderOut(nil)
        backdropWindows.forEach { $0.orderOut(nil) }
        toolbarWindow = nil
        canvasWindow = nil
        backdropWindows.removeAll()
    }

    private func selectTool(_ tool: ScreenshotAnnotationTool) {
        annotationView.tool = tool
        toolControl?.selectedSegment = tool.rawValue
        annotationView.window?.invalidateCursorRects(for: annotationView)
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
    private let clearRect: NSRect?

    init(frame: NSRect, clearRect: NSRect?) {
        self.clearRect = clearRect
        super.init(frame: frame)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func draw(_ dirtyRect: NSRect) {
        let mask = NSBezierPath(rect: bounds)
        if let clearRect, !clearRect.isEmpty {
            mask.appendRect(clearRect)
            mask.windingRule = .evenOdd
        }
        NSColor.black.withAlphaComponent(0.38).setFill()
        mask.fill()
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func mouseDown(with event: NSEvent) {
        onCancel?()
    }
}
