import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentChatModelClientTests: XCTestCase {
    func testResponsesUsesCompactionAndStatelessFullOutputContinuation() async throws {
        let recorder = ResponsesTransportRecorder()
        let client = try AgentResponsesModelClient(
            baseURL: URL(string: "https://api.openai.com/v1/chat/completions")!,
            model: "gpt-test", apiKey: "secret", maximumOutputTokens: 1_200,
            temperature: 0, transport: { request in try await recorder.send(request) }
        )
        let initial: [AgentMessage] = [
            .init(role: .system, content: "Use tools"),
            .init(role: .user, content: "Start"),
        ]
        let first = try await client.complete(
            messages: initial, tools: runtimeTestTools, timeout: 20
        )
        let second = try await client.complete(
            messages: initial + [first, .init(role: .tool, content: "done", toolCallID: "call_1")],
            tools: runtimeTestTools, timeout: 20
        )

        XCTAssertEqual(second.toolCalls.first?.name, "finish")
        let requests = await recorder.requests
        XCTAssertEqual(requests.count, 2)
        XCTAssertEqual(requests[0].url?.absoluteString, "https://api.openai.com/v1/responses")
        let firstBody = try Self.body(requests[0])
        XCTAssertEqual(firstBody["max_output_tokens"] as? Int, 1_200)
        XCTAssertNil(firstBody["max_tokens"])
        XCTAssertEqual(
            ((firstBody["context_management"] as? [[String: Any]])?.first?["compact_threshold"] as? Int),
            200_000
        )
        XCTAssertEqual(
            ((firstBody["tools"] as? [[String: Any]])?.first?["name"] as? String),
            "work"
        )
        let secondBody = try Self.body(requests[1])
        XCTAssertNil(secondBody["previous_response_id"])
        let history = try XCTUnwrap(secondBody["input"] as? [[String: Any]])
        XCTAssertEqual(history.count, 4)
        XCTAssertEqual(history[2]["type"] as? String, "function_call")
        XCTAssertEqual(history[2]["call_id"] as? String, "call_1")
        XCTAssertEqual(history[3]["type"] as? String, "function_call_output")
        XCTAssertEqual(history[3]["call_id"] as? String, "call_1")
        XCTAssertEqual(secondBody["prompt_cache_key"] as? String, "chatos-native:gpt-test")
    }

    func testEverySafeGatewaySelectsResponsesCompaction() {
        XCTAssertTrue(AgentResponsesModelClient.shouldUse(
            provider: "openai", baseURL: URL(string: "https://api.openai.com/v1")!
        ))
        XCTAssertTrue(AgentResponsesModelClient.shouldUse(
            provider: "openai", baseURL: URL(string: "https://relay.example/v1")!
        ))
        XCTAssertTrue(AgentResponsesModelClient.shouldUse(
            provider: "deepseek", baseURL: URL(string: "https://api.openai.com/v1")!
        ))
    }

    func testResponsesStreamingRequiresCompletedEventAndReturnsAuthoritativeToolCall() async throws {
        let sse = """
        data: {"type":"response.created","response":{"id":"resp_stream"}}

        data: {"type":"response.output_text.delta","delta":"checking"}

        data: {"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"call_stream","name":"finish","arguments":""}}

        data: {"type":"response.function_call_arguments.delta","output_index":0,"delta":"{}"}

        data: {"type":"response.completed","response":{"id":"resp_stream","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"checking"}]},{"type":"function_call","call_id":"call_stream","name":"finish","arguments":"{}"}]}}

        """
        let client = try AgentResponsesModelClient(
            baseURL: URL(string: "https://api.openai.com/v1")!, model: "gpt-test", apiKey: "secret",
            streamTransport: { _ in
                let pair = AsyncThrowingStream<Data, Error>.makeStream()
                pair.continuation.yield(Data(sse.utf8)); pair.continuation.finish()
                return .init(statusCode: 200, body: pair.stream)
            }
        )
        let events = StreamEventCollector()
        let message = try await client.stream(
            messages: [.init(role: .user, content: "finish")], tools: runtimeTestTools,
            timeout: 20, onEvent: { await events.append($0) }
        )
        XCTAssertEqual(message.content, "checking")
        XCTAssertEqual(
            message.toolCalls,
            [.init(id: "call_stream", name: "finish", arguments: "{}")]
        )
        let captured = await events.values
        XCTAssertEqual(captured.first, .responseCreated)
        XCTAssertEqual(captured.last, .completed)
        XCTAssertTrue(captured.contains(.textDelta("checking")))
    }

    func testResponsesStreamingReportsOfficialTerminalFailuresPrecisely() async throws {
        let cases: [(String, String)] = [
            (
                #"data: {"type":"response.incomplete","response":{"id":"resp_1","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}}}"# + "\n\n",
                "OpenAI Responses 明确返回 response.incomplete（原因：max_output_tokens）"
            ),
            (
                #"data: {"type":"response.failed","response":{"id":"resp_2","status":"failed","error":{"code":"server_error","message":"do-not-display-this-value"}}}"# + "\n\n",
                "OpenAI Responses 明确返回 response.failed（代码：server_error）"
            ),
            (
                #"data: {"type":"error","code":"rate_limit_exceeded","message":"do-not-display-this-value","param":null,"sequence_number":1}"# + "\n\n",
                "OpenAI Responses 数据流返回 error 事件（代码：rate_limit_exceeded）"
            ),
        ]
        for (sse, expectedPrefix) in cases {
            let client = try responsesStreamingClient(sse: sse)
            do {
                _ = try await client.stream(
                    messages: [.init(role: .user, content: "test")],
                    tools: runtimeTestTools,
                    timeout: 20,
                    onEvent: { _ in }
                )
                XCTFail("Official failure terminal must stop the run")
            } catch {
                XCTAssertTrue(error.localizedDescription.hasPrefix(expectedPrefix))
                XCTAssertFalse(error.localizedDescription.contains("do-not-display-this-value"))
            }
        }
    }

    func testResponsesStreamingRejectsMissingCompletionAndInvalidFunctionCall() async throws {
        let missingCompletion = try responsesStreamingClient(sse: """
        data: {"type":"response.created","response":{"id":"resp_partial","status":"in_progress"}}

        """)
        do {
            _ = try await missingCompletion.stream(
                messages: [.init(role: .user, content: "test")], tools: runtimeTestTools,
                timeout: 20, onEvent: { _ in }
            )
            XCTFail("A stream without response.completed must fail")
        } catch {
            guard case AgentRuntimeError.responsesStreamMissingCompletion = error else {
                return XCTFail("Expected missing completion, got \(error)")
            }
        }

        let invalidCall = try responsesStreamingClient(sse: """
        data: {"type":"response.completed","response":{"id":"resp_invalid","status":"completed","output":[{"type":"function_call","name":"finish","arguments":"{}"}]}}

        """)
        do {
            _ = try await invalidCall.stream(
                messages: [.init(role: .user, content: "test")], tools: runtimeTestTools,
                timeout: 20, onEvent: { _ in }
            )
            XCTFail("A function call without call_id must fail")
        } catch {
            guard case AgentRuntimeError.invalidResponsesFunctionCall = error else {
                return XCTFail("Expected invalid function call, got \(error)")
            }
        }
    }

    func testResponsesStreamingRejectsNonSSESuccessBody() async throws {
        let client = try AgentResponsesModelClient(
            baseURL: URL(string: "https://api.openai.com/v1")!,
            model: "gpt-test",
            apiKey: "secret",
            streamTransport: { _ in
                let pair = AsyncThrowingStream<Data, Error>.makeStream()
                pair.continuation.yield(Data(#"{"id":"resp_json","status":"completed","output":[]}"#.utf8))
                pair.continuation.finish()
                return .init(
                    statusCode: 200,
                    headers: ["content-type": "application/json"],
                    body: pair.stream
                )
            }
        )
        do {
            _ = try await client.stream(
                messages: [.init(role: .user, content: "test")], tools: runtimeTestTools,
                timeout: 20, onEvent: { _ in }
            )
            XCTFail("A streaming request must receive SSE")
        } catch {
            guard case AgentRuntimeError.responsesStreamUnexpectedContentType = error else {
                return XCTFail("Expected content type failure, got \(error)")
            }
        }
    }

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

    private static func body(_ request: URLRequest) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(request.httpBody)) as? [String: Any])
    }

    private func responsesStreamingClient(sse: String) throws -> AgentResponsesModelClient {
        try AgentResponsesModelClient(
            baseURL: URL(string: "https://api.openai.com/v1")!,
            model: "gpt-test",
            apiKey: "secret",
            streamTransport: { _ in
                let pair = AsyncThrowingStream<Data, Error>.makeStream()
                pair.continuation.yield(Data(sse.utf8))
                pair.continuation.finish()
                return .init(
                    statusCode: 200,
                    headers: ["content-type": "text/event-stream"],
                    body: pair.stream
                )
            }
        )
    }
}

private actor ResponsesTransportRecorder {
    var requests: [URLRequest] = []
    func send(_ request: URLRequest) throws -> (Data, Int) {
        requests.append(request)
        let call = requests.count == 1 ? ("resp_1", "call_1", "work") : ("resp_2", "call_2", "finish")
        let body: [String: Any] = [
            "id": call.0,
            "status": "completed",
            "output": [[
                "type": "function_call",
                "call_id": call.1,
                "name": call.2,
                "arguments": "{}",
            ]],
        ]
        return (try JSONSerialization.data(withJSONObject: body), 200)
    }
}

private actor StreamEventCollector {
    var values: [AgentModelStreamEvent] = []
    func append(_ value: AgentModelStreamEvent) { values.append(value) }
}
