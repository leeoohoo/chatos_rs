import AppKit
import SwiftUI

@MainActor
class GlobalCommandPanelController: NSWindowController, NSWindowDelegate {
    private var previousApplication: NSRunningApplication?
    private let panel: GlobalCommandPanel

    init(size: NSSize) {
        panel = GlobalCommandPanel(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        super.init(window: panel)

        panel.delegate = self
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = true
        panel.level = .floating
        panel.animationBehavior = .utilityWindow
        panel.collectionBehavior = [
            .canJoinAllSpaces,
            .fullScreenAuxiliary,
            .transient,
            .ignoresCycle,
        ]
        panel.becomesKeyOnlyIfNeeded = false
        panel.onCancel = { [weak self] in self?.closeAndRestorePreviousApplication() }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    var isPresented: Bool {
        panel.isVisible
    }

    func setRootView<Content: View>(_ view: Content) {
        let hosting = NSHostingView(rootView: view)
        hosting.sizingOptions = []
        hosting.frame = panel.contentView?.bounds ?? NSRect(origin: .zero, size: panel.frame.size)
        panel.contentView = hosting
    }

    func toggle() {
        isPresented ? closeAndRestorePreviousApplication() : present()
    }

    func present() {
        if NSWorkspace.shared.frontmostApplication?.bundleIdentifier
            != Bundle.main.bundleIdentifier {
            previousApplication = NSWorkspace.shared.frontmostApplication
        }
        positionOnMouseScreen()
        NSApp.activate(ignoringOtherApps: true)
        panel.makeKeyAndOrderFront(nil)
    }

    func closeAndRestorePreviousApplication() {
        panel.orderOut(nil)
        previousApplication?.activate()
        previousApplication = nil
    }

    func windowDidResignKey(_ notification: Notification) {
        guard panel.isVisible else { return }
        panel.orderOut(nil)
        previousApplication = nil
    }

    private func positionOnMouseScreen() {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first(where: { NSMouseInRect(mouse, $0.frame, false) })
            ?? NSScreen.main
            ?? NSScreen.screens.first
        guard let screen else { return }

        let visible = screen.visibleFrame
        let origin = NSPoint(
            x: visible.midX - panel.frame.width / 2,
            y: visible.maxY - visible.height * 0.18 - panel.frame.height / 2
        )
        panel.setFrameOrigin(origin)
    }
}

private final class GlobalCommandPanel: NSPanel {
    var onCancel: (() -> Void)?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }
}
