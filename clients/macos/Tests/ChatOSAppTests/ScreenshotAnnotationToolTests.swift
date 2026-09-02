import Testing
@testable import ChatOSApp

struct ScreenshotAnnotationToolTests {
    @Test
    func singleKeyShortcutsSelectEveryAnnotationTool() {
        #expect(ScreenshotAnnotationTool.shortcut("p") == .pen)
        #expect(ScreenshotAnnotationTool.shortcut("L") == .line)
        #expect(ScreenshotAnnotationTool.shortcut("r") == .rectangle)
        #expect(ScreenshotAnnotationTool.shortcut("O") == .ellipse)
        #expect(ScreenshotAnnotationTool.shortcut("a") == .arrow)
        #expect(ScreenshotAnnotationTool.shortcut("H") == .highlight)
        #expect(ScreenshotAnnotationTool.shortcut("m") == .mosaic)
        #expect(ScreenshotAnnotationTool.shortcut("T") == .text)
        #expect(ScreenshotAnnotationTool.shortcut("n") == .number)
    }

    @Test
    func numericShortcutsFollowToolbarOrder() {
        for index in 1...9 {
            #expect(ScreenshotAnnotationTool.shortcut(String(index))?.rawValue == index - 1)
        }
        #expect(ScreenshotAnnotationTool.shortcut("0") == nil)
        #expect(ScreenshotAnnotationTool.shortcut("x") == nil)
    }
}
