import AppKit
import Testing
@testable import ChatOSApp

@MainActor
struct ScreenshotOverlayWindowTests {
    @Test
    func screenshotPanelKeepsScreenSaverLevelAfterFloatingConfiguration() {
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 100, height: 100),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )

        configureScreenshotOverlayPanel(panel)

        #expect(panel.isFloatingPanel)
        #expect(panel.level == .screenSaver)
        #expect(panel.level.rawValue > NSWindow.Level.popUpMenu.rawValue)
    }

    @Test
    func annotationWindowsPreserveTheirLevelOrdering() {
        let backdrop = NSPanel()
        let canvas = NSPanel()
        let toolbar = NSPanel()

        configureScreenshotOverlayPanel(backdrop, levelOffset: 0)
        configureScreenshotOverlayPanel(canvas, levelOffset: 1)
        configureScreenshotOverlayPanel(toolbar, levelOffset: 2)

        #expect(backdrop.level == .screenSaver)
        #expect(canvas.level.rawValue == backdrop.level.rawValue + 1)
        #expect(toolbar.level.rawValue == canvas.level.rawValue + 1)
    }
}
