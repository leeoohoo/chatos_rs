import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentResponsesReasoningTests: XCTestCase {
    func testKnownModelRepairsLegacyEffortsAndPreservesSupportedValues() async throws {
        for (selected, expected) in [("none", "low"), ("minimal", "low"), ("low", "low"),
                                      ("medium", "medium"), ("high", "high"), ("xhigh", "xhigh"), ("max", "max")] {
            let client = try AgentResponsesModelClient(
                baseURL: URL(string: "https://example.test/v1")!, model: "gpt-6.1-sol",
                apiKey: "test-only", thinking: selected, transport: { request in
                    let body = try XCTUnwrap(request.httpBody)
                    let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
                    XCTAssertEqual((payload["reasoning"] as? [String: String])?["effort"], expected)
                    return (Data(#"{"id":"response","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"你好"}]}]}"#.utf8), 200)
                }
            )
            let result = try await client.complete(messages: [.init(role: .user, content: "你好")], tools: [], timeout: 10)
            XCTAssertEqual(result.content, "你好")
        }
    }
}
