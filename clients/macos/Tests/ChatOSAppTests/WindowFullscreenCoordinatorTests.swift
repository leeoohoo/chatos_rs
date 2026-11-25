import AppKit
import XCTest
@testable import ChatOSApp

@MainActor
final class WindowFullscreenCoordinatorTests: XCTestCase {
    func testFullscreenRoundTripRestoresFrameAndResizeConstraints() {
        let originalFrame = NSRect(x: 120, y: 90, width: 760, height: 560)
        let targetFrame = NSRect(x: 0, y: 25, width: 1_920, height: 1_055)
        let window = NSWindow(
            contentRect: originalFrame,
            styleMask: [.borderless, .resizable],
            backing: .buffered,
            defer: false
        )
        window.setFrame(originalFrame, display: false)
        window.contentMinSize = NSSize(width: 520, height: 360)
        window.contentMaxSize = NSSize(width: 1_200, height: 920)
        let coordinator = WindowFullscreenCoordinator()
        coordinator.attach(window)

        coordinator.enterFullScreen(in: targetFrame)

        XCTAssertTrue(coordinator.isFullScreen)
        XCTAssertEqual(window.frame, targetFrame)
        XCTAssertEqual(window.contentMinSize, .zero)
        XCTAssertEqual(window.contentMaxSize, targetFrame.size)

        coordinator.exitFullScreen()

        XCTAssertFalse(coordinator.isFullScreen)
        XCTAssertEqual(window.frame, originalFrame)
        XCTAssertEqual(window.contentMinSize, NSSize(width: 520, height: 360))
        XCTAssertEqual(window.contentMaxSize, NSSize(width: 1_200, height: 920))
    }
}
