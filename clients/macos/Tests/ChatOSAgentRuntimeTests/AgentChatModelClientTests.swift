import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentChatModelClientTests: XCTestCase {
    func testWireAdapterKeepsToolCallIDsAndHonorsOutputReserve() async throws {
        let client = try AgentChatModelClient(baseURL: URL(string: "https://model.example/prefix/v1/responses")!, model: "test", apiKey: "secret",
            maximumOutputTokens: 1_200, temperature: 0, transport: { request in
                XCTAssertEqual(request.url?.absoluteString, "https://model.example/prefix/v1/chat/completions")
                XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer secret")
                let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(request.httpBody)) as? [String: Any])
                XCTAssertEqual(payload["max_tokens"] as? Int, 1_200)
                XCTAssertEqual(payload["tool_choice"] as? String, "auto")
                let messages = try XCTUnwrap(payload["messages"] as? [[String: Any]])
                XCTAssertEqual(messages.last?["tool_call_id"] as? String, "original")
                return (Data(#"{"choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[{"id":"next","function":{"name":"finish","arguments":"{}"}}]}}]}"#.utf8), 200)
            })
        let result = try await client.complete(messages: [.init(role: .assistant, toolCalls: [.init(id: "original", name: "work", arguments: "{}")]),
                                                          .init(role: .tool, content: "result", toolCallID: "original")], tools: runtimeTestTools, timeout: 20)
        XCTAssertEqual(result.toolCalls.first?.id, "next")
    }

    func testOverflowIsRecognizedButOther400ErrorsAreNotRetriedAsCompaction() async throws {
        for code in ["context_length_exceeded", "invalid_model"] {
            let client = try AgentChatModelClient(baseURL: URL(string: "https://model.example/v1")!, model: "test", apiKey: "secret", transport: { _ in
                (Data("{\"error\":{\"code\":\"\(code)\"}}".utf8), 400)
            })
            do { _ = try await client.complete(messages: [], tools: [], timeout: 1); XCTFail("Should fail") }
            catch {
                if code == "context_length_exceeded" {
                    guard case AgentRuntimeError.contextOverflow = error else { return XCTFail("Expected overflow") }
                } else {
                    guard case AgentRuntimeError.provider(400) = error else { return XCTFail("Keep unrelated provider error") }
                }
            }
        }
    }

    func testTruncatedOutputCannotBecomeToolExecution() async throws {
        let client = try AgentChatModelClient(baseURL: URL(string: "https://model.example/v1")!, model: "test", apiKey: "secret", transport: { _ in
            (Data(#"{"choices":[{"finish_reason":"length","message":{"tool_calls":[]}}]}"#.utf8), 200)
        })
        do { _ = try await client.complete(messages: [], tools: [], timeout: 1); XCTFail("Reject truncated output") }
        catch { guard case AgentRuntimeError.invalidResponse = error else { return XCTFail("Expected invalid output") } }
    }
}
