import AppKit
import CoreGraphics

@MainActor
enum ScreenshotWindowExclusion {
    static func visibleOverlayWindowIDs(additionalWindows: [NSWindow] = []) -> [CGWindowID] {
        let overlayWindows = NSApp.windows.filter {
            $0.isVisible
                && $0.windowNumber > 0
                && $0.level.rawValue >= NSWindow.Level.floating.rawValue
        }
        return Array(Set((overlayWindows + additionalWindows).compactMap {
            $0.windowNumber > 0 ? CGWindowID($0.windowNumber) : nil
        }))
    }
}
