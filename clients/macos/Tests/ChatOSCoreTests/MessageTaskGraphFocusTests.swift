import ChatOSCore
import XCTest

final class MessageTaskGraphFocusTests: XCTestCase {
    func testFocusIncludesEntireUpstreamAndDownstreamPathButDimsSiblingBranch() throws {
        let edges = [
            edge("prepare", "implement"),
            edge("implement", "selected"),
            edge("selected", "release"),
            edge("release", "finish"),
            edge("implement", "sibling"),
        ]

        let context = try XCTUnwrap(
            MessageTaskGraphFocus.context(selectedTaskID: "selected", edges: edges)
        )

        XCTAssertEqual(
            context.relatedTaskIDs,
            Set(["prepare", "implement", "selected", "release", "finish"])
        )
        XCTAssertFalse(context.relatedTaskIDs.contains("sibling"))
        XCTAssertEqual(context.directTaskIDs, Set(["implement", "selected", "release"]))
    }

    private func edge(_ source: String, _ target: String) -> MessageTaskGraphEdge {
        MessageTaskGraphEdge(
            id: "\(source)->\(target)",
            sourceID: source,
            targetID: target
        )
    }
}
