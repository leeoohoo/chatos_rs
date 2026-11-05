import AppKit
import OSLog

/// Content-free diagnostics distinguish an ordered-out window from a renderer
/// failure when the app process remains alive. Do not log chat titles or text.
@MainActor
final class MainWorkspaceWindow: NSWindow {
    private static let logger = Logger(subsystem: "com.chatos.swift-client", category: "MainWindow")

    func startPresentationDiagnostics() {
        for name in [NSWindow.willBeginSheetNotification, NSWindow.didEndSheetNotification,
                     NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification,
                     NSWindow.didChangeOcclusionStateNotification, NSWindow.didEnterFullScreenNotification,
                     NSWindow.didExitFullScreenNotification, NSWindow.willEnterFullScreenNotification] {
            NotificationCenter.default.addObserver(self, selector: #selector(presentationChanged(_:)), name: name, object: nil)
        }
        NSWorkspace.shared.notificationCenter.addObserver(
            self, selector: #selector(presentationChanged(_:)),
            name: NSWorkspace.activeSpaceDidChangeNotification, object: nil
        )
    }

    @objc private func presentationChanged(_ notification: Notification) {
        if let window = notification.object as? NSWindow,
           window !== self, window.sheetParent !== self, window.parent !== self { return }
        if notification.name == NSWindow.willEnterFullScreenNotification,
           notification.object as? NSWindow === self {
            collectionBehavior.remove(.moveToActiveSpace)
        }
        record(notification.name.rawValue)
    }

    override func orderOut(_ sender: Any?) {
        record("orderOut")
        super.orderOut(sender)
    }

    override func close() {
        record("close")
        super.close()
    }

    override func miniaturize(_ sender: Any?) {
        record("miniaturize")
        super.miniaturize(sender)
    }

    override func makeKeyAndOrderFront(_ sender: Any?) {
        super.makeKeyAndOrderFront(sender)
        record("show")
    }

    func record(_ event: String) {
        let front = NSWorkspace.shared.frontmostApplication
        Self.logger.notice("event=\(event, privacy: .public) window=\(self.windowNumber) visible=\(self.isVisible) activeSpace=\(self.isOnActiveSpace) unoccluded=\(self.occlusionState.contains(.visible)) minimized=\(self.isMiniaturized) fullscreen=\(self.styleMask.contains(.fullScreen)) behavior=\(self.collectionBehavior.rawValue) active=\(NSApp.isActive) hidden=\(NSApp.isHidden) policy=\(NSApp.activationPolicy().rawValue) key=\(NSApp.keyWindow?.windowNumber ?? -1) main=\(NSApp.mainWindow?.windowNumber ?? -1) sheet=\(self.attachedSheet?.windowNumber ?? -1) frontPID=\(front?.processIdentifier ?? -1) frontBundle=\(front?.bundleIdentifier ?? "unknown", privacy: .public)")
        if let sheet = attachedSheet {
            Self.logger.notice("sheetWindow=\(sheet.windowNumber) parent=\(sheet.sheetParent?.windowNumber ?? -1) visible=\(sheet.isVisible) activeSpace=\(sheet.isOnActiveSpace) unoccluded=\(sheet.occlusionState.contains(.visible)) frame=\(NSStringFromRect(sheet.frame), privacy: .public)")
        }
    }
}
