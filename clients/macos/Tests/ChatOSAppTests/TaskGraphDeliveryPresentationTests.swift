import XCTest
@testable import ChatOSApp

final class TaskGraphDeliveryPresentationTests: XCTestCase {
    func testShortDeliveryStaysExpanded() {
        XCTAssertFalse(TaskGraphDeliveryPresentation.shouldCollapse("完成。"))
        XCTAssertEqual(TaskGraphDeliveryPresentation.preview(" 完成。 \n"), "完成。")
    }

    func testLongDeliveryUsesBoundedPreview() {
        let text = String(repeating: "结果", count: 700)

        XCTAssertTrue(TaskGraphDeliveryPresentation.shouldCollapse(text))
        XCTAssertLessThanOrEqual(
            TaskGraphDeliveryPresentation.preview(text).count,
            TaskGraphDeliveryPresentation.previewCharacterLimit + 1
        )
        XCTAssertTrue(TaskGraphDeliveryPresentation.preview(text).hasSuffix("…"))
    }
}
