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

    @Test
    func frozenCaptureCropUsesDisplayPointToPixelScale() throws {
        let image = try #require(Self.makeImage(width: 400, height: 200))

        let crop = try #require(ScreenSelectionOverlayController.crop(
            image,
            to: CGRect(x: 25, y: 10, width: 50, height: 40),
            displaySize: CGSize(width: 200, height: 100)
        ))

        #expect(crop.width == 100)
        #expect(crop.height == 80)
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

    private static func makeImage(width: Int, height: Int) -> CGImage? {
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        guard let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return nil }
        return context.makeImage()
    }
}
