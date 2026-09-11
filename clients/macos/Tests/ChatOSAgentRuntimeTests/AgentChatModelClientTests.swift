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

    func testSafeProviderFailureReasonExplainsOverloadedRelayWithoutEchoingRawBody() async throws {
        let rawSecret = "do-not-display-this-value"
        let body = Data(#"{"error":{"type":"server_error","code":"internal_server_error","message":"auth_unavailable: no auth available; server_is_overloaded: Our servers are currently overloaded. do-not-display-this-value"}}"#.utf8)
        let client = try AgentChatModelClient(
            baseURL: URL(string: "https://model.example/v1")!, model: "test", apiKey: "secret",
            transport: { _ in (body, 503) }
        )
        do {
            _ = try await client.complete(messages: [], tools: [], timeout: 1)
            XCTFail("Should fail")
        } catch {
            guard case AgentRuntimeError.providerDetail(503, let detail) = error else {
                return XCTFail("Expected a categorized provider failure")
            }
            XCTAssertTrue(detail.contains("上游模型处于过载状态"))
            XCTAssertFalse(error.localizedDescription.contains(rawSecret))
        }
    }

    func testTruncatedOutputCannotBecomeToolExecution() async throws {
        let client = try AgentChatModelClient(baseURL: URL(string: "https://model.example/v1")!, model: "test", apiKey: "secret", transport: { _ in
            (Data(#"{"choices":[{"finish_reason":"length","message":{"tool_calls":[]}}]}"#.utf8), 200)
        })
        do { _ = try await client.complete(messages: [], tools: [], timeout: 1); XCTFail("Reject truncated output") }
        catch { guard case AgentRuntimeError.invalidResponse = error else { return XCTFail("Expected invalid output") } }
    }

    func testStreamingReassemblesTextAndFragmentedToolArguments() async throws {
        let raw = """
        data: {"choices":[{"delta":{"content":"正在分析"}}]}

        data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"story_finish","arguments":"{\\\"ok\\\":"}}]}}]}

        data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"true}"}}]},"finish_reason":"tool_calls"}]}

        data: [DONE]

        """
        let bytes = Data(raw.utf8)
        let cuts = [7, 31, 83, 127, bytes.count].filter { $0 < bytes.count } + [bytes.count]
        var start = 0
        let chunks = cuts.map { end -> Data in defer { start = end }; return bytes.subdata(in: start..<end) }
        let events = StreamEventCollector()
        let client = try AgentChatModelClient(baseURL: URL(string: "https://model.example/v1")!, model: "test", apiKey: "secret",
            transport: { _ in throw AgentRuntimeError.invalidResponse }, streamTransport: { request in
                let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(request.httpBody)) as? [String: Any])
                XCTAssertEqual(payload["stream"] as? Bool, true)
                XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "text/event-stream")
                let pair = AsyncThrowingStream<Data, Error>.makeStream()
                for chunk in chunks { pair.continuation.yield(chunk) }
                pair.continuation.finish()
                return AgentHTTPStreamResponse(statusCode: 200, body: pair.stream)
            })
        let message = try await client.stream(messages: [AgentMessage(role: .user, content: "规划")], tools: runtimeTestTools,
                                              timeout: 10, onEvent: { await events.append($0) })
        XCTAssertEqual(message.content, "正在分析")
        XCTAssertEqual(message.toolCalls, [AgentToolCall(id: "call_1", name: "story_finish", arguments: #"{"ok":true}"#)])
        let captured = await events.values
        XCTAssertEqual(captured.first, .responseCreated)
        XCTAssertEqual(captured.last, .completed)
        XCTAssertTrue(captured.contains(.textDelta("正在分析")))
    }

    func testIncompleteStreamNeverReturnsExecutableToolCall() async throws {
        let client = try AgentChatModelClient(baseURL: URL(string: "https://model.example/v1")!, model: "test", apiKey: "secret",
            streamTransport: { _ in
                let pair = AsyncThrowingStream<Data, Error>.makeStream()
                let partial = #"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"paid","function":{"name":"generate","arguments":"{"}}]}}]}"# + "\n\n"
                pair.continuation.yield(Data(partial.utf8)); pair.continuation.finish()
                return AgentHTTPStreamResponse(statusCode: 200, body: pair.stream)
            })
        do {
            _ = try await client.stream(messages: [], tools: [], timeout: 10, onEvent: { _ in })
            XCTFail("An interrupted stream cannot execute a partially assembled tool call")
        } catch { guard case AgentRuntimeError.invalidResponse = error else { return XCTFail("Expected invalid stream") } }
    }
}

private actor StreamEventCollector {
    var values: [AgentModelStreamEvent] = []
    func append(_ value: AgentModelStreamEvent) { values.append(value) }
}
