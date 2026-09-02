import AppKit
import SwiftUI

@MainActor
class GlobalCommandPanelController: NSWindowController, NSWindowDelegate {
    var onPanelDismiss: (() -> Void)?
    private var previousApplication: NSRunningApplication?
    private weak var previousKeyWindow: NSWindow?
    private let panel: GlobalCommandPanel
    private var shouldFocusFirstTextInput = false

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
        // Global utility panels are intentionally shown while another app remains
        // active. Hiding on deactivation makes them visible only over Finder/the
        // desktop, and the floating level is not reliable over another process.
        panel.hidesOnDeactivate = false
        panel.level = .popUpMenu
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
        hosting.autoresizingMask = [.width, .height]
        panel.contentView = hosting
    }

    func setContentSize(_ size: NSSize) {
        guard panel.contentRect(forFrameRect: panel.frame).size != size else { return }
        let previousFrame = panel.frame
        panel.setContentSize(size)
        panel.setFrameOrigin(NSPoint(
            x: previousFrame.midX - panel.frame.width / 2,
            y: previousFrame.maxY - panel.frame.height
        ))
    }

    func toggle() {
        isPresented ? closeAndRestorePreviousApplication() : present()
    }

    func present(focusingFirstTextInput: Bool = false) {
        shouldFocusFirstTextInput = focusingFirstTextInput
        if NSWorkspace.shared.frontmostApplication?.bundleIdentifier
            != Bundle.main.bundleIdentifier {
            previousApplication = NSWorkspace.shared.frontmostApplication
            previousKeyWindow = nil
        } else {
            previousApplication = nil
            previousKeyWindow = NSApp.keyWindow === panel ? nil : NSApp.keyWindow
        }
        positionOnMouseScreen()
        panel.contentView?.layoutSubtreeIfNeeded()
        if focusingFirstTextInput {
            focusFirstTextInputIfAvailable()
        }
        // Activate ChatOS for the lifetime of the command panel, then restore the
        // original application when the user submits or dismisses it.
        NSApp.activate(ignoringOtherApps: true)
        panel.makeKeyAndOrderFront(nil)
        if focusingFirstTextInput {
            focusFirstTextInputIfAvailable()
            DispatchQueue.main.async { [weak self] in
                guard let self, self.panel.isVisible, self.shouldFocusFirstTextInput else { return }
                self.panel.contentView?.layoutSubtreeIfNeeded()
                self.focusFirstTextInputIfAvailable()
            }
        }
    }

    func closeAndRestorePreviousApplication(afterRestoring completion: (() -> Void)? = nil) {
        let application = previousApplication
        let keyWindow = previousKeyWindow
        panel.orderOut(nil)
        shouldFocusFirstTextInput = false
        previousApplication = nil
        previousKeyWindow = nil

        if let application, !application.isTerminated, !application.isActive {
            application.activate()
        } else if application == nil {
            keyWindow?.makeKey()
        }

        guard let completion else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) {
            completion()
        }
    }

    func closeWithoutRestoringPreviousApplication() {
        panel.orderOut(nil)
        shouldFocusFirstTextInput = false
        previousApplication = nil
        previousKeyWindow = nil
    }

    func windowDidBecomeKey(_ notification: Notification) {
        guard panel.isVisible, shouldFocusFirstTextInput else { return }
        panel.contentView?.layoutSubtreeIfNeeded()
        focusFirstTextInputIfAvailable()
    }

    func windowDidResignKey(_ notification: Notification) {
        guard panel.isVisible else { return }
        panel.orderOut(nil)
        shouldFocusFirstTextInput = false
        previousApplication = nil
        previousKeyWindow = nil
        onPanelDismiss?()
    }

    @discardableResult
    func focusFirstTextInputIfAvailable() -> Bool {
        guard let contentView = panel.contentView,
              let field = searchField(in: contentView) else {
            return false
        }
        panel.initialFirstResponder = field
        guard panel.isKeyWindow else { return true }
        return panel.makeFirstResponder(field)
    }

    private func searchField(in view: NSView) -> NSTextField? {
        if let field = view as? NSTextField,
           field.identifier == GlobalCommandSearchField.focusIdentifier {
            return field
        }
        for subview in view.subviews {
            if let field = searchField(in: subview) {
                return field
            }
        }
        return nil
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
