import AppKit
import ChatOSConnector
import CoreGraphics

@MainActor
final class LongScreenshotCaptureController {
    var onComplete: ((CGImage) -> Void)?
    var onCancel: (() -> Void)?

    private let selection: ScreenSelection
    private let initialImage: CGImage
    private let isEnglish: Bool
    private let captureService = NativeScreenCaptureService()
    private let composer = NativeLongScreenshotComposer()
    private var controlPanel: NSPanel?
    private var captureTask: Task<Void, Never>?
    private weak var statusLabel: NSTextField?
    private weak var doneButton: NSButton?
    private var capturedSections = 1
    private var isFinishing = false

    init(initialImage: CGImage, selection: ScreenSelection, isEnglish: Bool) {
        self.initialImage = initialImage
        self.selection = selection
        self.isEnglish = isEnglish
    }

    func present() {
        guard controlPanel == nil else { return }
        presentControlPanel()
        captureTask = Task { [weak self] in
            guard let self else { return }
            do {
                try await self.composer.start(with: self.initialImage)
                self.statusLabel?.stringValue = self.localized(
                    "滚动页面，完成后点对号",
                    "Scroll the page, then click the checkmark"
                )
                while !Task.isCancelled, !self.isFinishing {
                    try await Task.sleep(for: .milliseconds(420))
                    try Task.checkCancellation()
                    let frame = try await self.captureService.capture(
                        region: self.captureRegionExcludingControls()
                    )
                    try Task.checkCancellation()
                    let result = try await self.composer.append(frame)
                    self.handle(result)
                }
            } catch is CancellationError {
                return
            } catch {
                self.statusLabel?.stringValue = self.localized(
                    "采集暂停：\(error.localizedDescription)",
                    "Capture paused: \(error.localizedDescription)"
                )
            }
        }
    }

    func cancel() {
        guard !isFinishing else { return }
        isFinishing = true
        captureTask?.cancel()
        captureTask = nil
        dismissPanel()
        onCancel?()
        clearCallbacks()
    }

    private func complete() {
        guard !isFinishing else { return }
        isFinishing = true
        doneButton?.isEnabled = false
        statusLabel?.stringValue = localized("正在生成长截图…", "Composing long screenshot…")
        captureTask?.cancel()
        captureTask = nil
        Task { [weak self] in
            guard let self else { return }
            do {
                let image = try await self.composer.outputImage()
                self.dismissPanel()
                self.onComplete?(image)
                self.clearCallbacks()
            } catch {
                self.isFinishing = false
                self.doneButton?.isEnabled = true
                self.statusLabel?.stringValue = self.localized(
                    "生成失败：\(error.localizedDescription)",
                    "Composition failed: \(error.localizedDescription)"
                )
            }
        }
    }

    private func presentControlPanel() {
        let size = NSSize(width: 370, height: 54)
        let panel = ScreenshotLongCapturePanel(
            contentRect: NSRect(origin: panelOrigin(for: size), size: size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false,
            screen: selection.screen
        )
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        panel.level = NSWindow.Level(rawValue: NSWindow.Level.screenSaver.rawValue + 2)
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        panel.isFloatingPanel = true
        panel.hidesOnDeactivate = false
        panel.contentView = makeControlView(size: size)
        panel.orderFrontRegardless()
        controlPanel = panel
    }

    private func makeControlView(size: NSSize) -> NSView {
        let material = NSVisualEffectView(frame: NSRect(origin: .zero, size: size))
        material.material = .hudWindow
        material.blendingMode = .behindWindow
        material.state = .active
        material.wantsLayer = true
        material.layer?.cornerRadius = 13
        material.layer?.masksToBounds = true

        let icon = NSImageView(image: symbol(
            "arrow.down.to.line",
            description: localized("长截图", "Long screenshot")
        ))
        icon.contentTintColor = .controlAccentColor

        let label = NSTextField(labelWithString: localized(
            "正在准备长截图…",
            "Preparing long screenshot…"
        ))
        label.font = .systemFont(ofSize: 12, weight: .medium)
        label.textColor = .labelColor
        label.lineBreakMode = .byTruncatingTail
        statusLabel = label

        let cancel = iconButton(
            "xmark",
            help: localized("取消长截图", "Cancel long screenshot"),
            action: #selector(cancelPressed)
        )
        let done = iconButton(
            "checkmark",
            help: localized("结束并保存", "Finish and save"),
            action: #selector(donePressed)
        )
        done.contentTintColor = .systemGreen
        doneButton = done

        let stack = NSStackView(views: [icon, label, cancel, done])
        stack.orientation = .horizontal
        stack.alignment = .centerY
        stack.spacing = 10
        stack.translatesAutoresizingMaskIntoConstraints = false
        material.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: material.leadingAnchor, constant: 14),
            stack.trailingAnchor.constraint(equalTo: material.trailingAnchor, constant: -10),
            stack.centerYAnchor.constraint(equalTo: material.centerYAnchor),
            label.widthAnchor.constraint(greaterThanOrEqualToConstant: 220),
        ])
        return material
    }

    private func handle(_ result: NativeLongScreenshotAppendResult) {
        switch result {
        case let .appended(newRows, totalHeight):
            capturedSections += 1
            statusLabel?.stringValue = localized(
                "已拼接 \(capturedSections) 段 · +\(newRows) px · 共 \(totalHeight) px",
                "\(capturedSections) sections · +\(newRows) px · \(totalHeight) px total"
            )
        case let .unchanged(totalHeight):
            statusLabel?.stringValue = localized(
                "继续向下滚动 · 当前 \(totalHeight) px",
                "Keep scrolling down · \(totalHeight) px"
            )
        case let .overlapNotFound(totalHeight):
            statusLabel?.stringValue = localized(
                "未识别到重叠，请放慢滚动 · 当前 \(totalHeight) px",
                "Overlap not found; scroll more slowly · \(totalHeight) px"
            )
        }
    }

    private func captureRegionExcludingControls() -> NativeScreenCaptureRegion {
        NativeScreenCaptureRegion(
            displayID: selection.captureRegion.displayID,
            sourceRect: selection.captureRegion.sourceRect,
            outputSize: selection.captureRegion.outputSize,
            excludedWindowIDs: controlPanel.map { [CGWindowID($0.windowNumber)] } ?? []
        )
    }

    private func panelOrigin(for size: NSSize) -> NSPoint {
        let visible = selection.screen.visibleFrame
        let x = min(
            max(selection.globalRect.midX - size.width / 2, visible.minX + 8),
            visible.maxX - size.width - 8
        )
        let below = selection.globalRect.minY - size.height - 10
        if below >= visible.minY + 8 {
            return NSPoint(x: x, y: below)
        }
        return NSPoint(
            x: x,
            y: min(selection.globalRect.maxY + 10, visible.maxY - size.height - 8)
        )
    }

    private func dismissPanel() {
        controlPanel?.orderOut(nil)
        controlPanel = nil
    }

    private func clearCallbacks() {
        onComplete = nil
        onCancel = nil
    }

    @objc private func cancelPressed() {
        cancel()
    }

    @objc private func donePressed() {
        complete()
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

private final class ScreenshotLongCapturePanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}
