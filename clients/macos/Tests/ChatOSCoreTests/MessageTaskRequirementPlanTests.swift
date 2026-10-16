import ChatOSCore
import Foundation
import XCTest

final class MessageTaskRequirementPlanTests: XCTestCase {
    func testDisplaysOnlyTaskOwnFrozenVersion() throws {
        let value: [String: Any] = ["requirement_plan": ["snapshot": [
            "plan_id": "plan-1", "revision": 1, "title": "Original proposal", "markdown": "Frozen original body",
            "confirmed_at_unix_ms": 10,
        ], "markdown_sha256": String(repeating: "a", count: 64)]]
        let text = String(decoding: try JSONSerialization.data(withJSONObject: value), as: UTF8.self)
        let plan = try XCTUnwrap(MessageTaskRequirementPlan.decode(inputPayloadJSON: text))
        XCTAssertEqual(plan.snapshot.revision, 1)
        XCTAssertEqual(plan.snapshot.markdown, "Frozen original body")
    }
    func testLegacyMalformedUnconfirmedAndZeroRevisionDoNotProduceAProposal() {
        for input in [nil, "{}", "not json", #"{"requirement_plan":{"snapshot":{"revision":1,"markdown":"draft"}}}"#,
            #"{"requirement_plan":{"snapshot":{"plan_id":"p","revision":0,"title":"t","markdown":"m","confirmed_at_unix_ms":1},"markdown_sha256":"bad"}}}"#] {
            XCTAssertNil(MessageTaskRequirementPlan.decode(inputPayloadJSON: input))
        }
    }
}
