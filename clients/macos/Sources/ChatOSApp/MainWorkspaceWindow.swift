import AppKit
import OSLog

/// Content-free diagnostics distinguish an ordered-out window from a renderer
/// failure when the app process remains alive. Do not log chat titles or text.
@MainActor
final class MainWorkspaceWindow: NSWindow {
    private static let logger = Logger(subsystem: "com.chatos.swift-client", category: "MainWindow")

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
        Self.logger.notice("event=\(event, privacy: .public) visible=\(self.isVisible) minimized=\(self.isMiniaturized) fullscreen=\(self.styleMask.contains(.fullScreen)) active=\(NSApp.isActive) hidden=\(NSApp.isHidden)")
    }
}
