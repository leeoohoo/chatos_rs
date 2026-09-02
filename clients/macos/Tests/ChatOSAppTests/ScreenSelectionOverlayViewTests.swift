import AppKit
import Testing
@testable import ChatOSApp

@MainActor
struct ScreenSelectionOverlayViewTests {
    @Test
    func regionModeCompletesOnNormalMouseUp() throws {
        let view = ScreenSelectionOverlayView(isEnglish: false)
        var completionCount = 0
        view.onSelectionCompleted = { _, _ in completionCount += 1 }

        view.mouseUp(with: try mouseUp(clickCount: 1))

        #expect(completionCount == 1)
    }

    private func mouseUp(clickCount: Int) throws -> NSEvent {
        try #require(NSEvent.mouseEvent(
            with: .leftMouseUp,
            location: NSPoint(x: 20, y: 20),
            modifierFlags: [],
            timestamp: 0,
            windowNumber: 0,
            context: nil,
            eventNumber: 1,
            clickCount: clickCount,
            pressure: 0
        ))
    }
}
