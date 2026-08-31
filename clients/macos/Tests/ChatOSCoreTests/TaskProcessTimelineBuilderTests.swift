import ChatOSCore
import XCTest

final class TaskProcessTimelineBuilderTests: XCTestCase {
    func testBuildsStructuredTimelineFromProcessLog() {
        let items = TaskProcessTimelineBuilder.build(
            processLog: """
            [2026-08-24T10:00:00Z] 开始访问
            检查窗口是否可读。
            [2026-08-24T10:01:00Z] 访问受阻
            没有辅助功能权限。
            """,
            taskStatus: "blocked"
        )

        XCTAssertEqual(items.count, 2)
        XCTAssertEqual(items[0].title, "开始访问")
        XCTAssertEqual(items[0].detail, "检查窗口是否可读。")
        XCTAssertEqual(items[0].status, "succeeded")
        XCTAssertEqual(items[1].title, "访问受阻")
        XCTAssertEqual(items[1].status, "blocked")
    }

    func testReturnsEmptyTimelineWithoutProcessLog() {
        XCTAssertTrue(
            TaskProcessTimelineBuilder.build(processLog: nil, taskStatus: "running").isEmpty
        )
    }
}
