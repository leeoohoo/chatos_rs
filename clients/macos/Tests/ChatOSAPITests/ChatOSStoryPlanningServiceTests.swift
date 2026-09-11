import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSStoryPlanningServiceTests: XCTestCase {
    func testAgentFactoryUsesAutomaticFunctionChoiceAndVisibleOutputReserve() async throws {
        let transport = StoryPlanningTransport()
        var policy = AgentRunPolicy(); var context = AgentContextPolicy(); context.outputReserveTokens = 2_048; policy.context = context
        let model = try await makeService(transport).makeAgentModel(configID: "text-model", policy: policy)
        let definition = AgentToolDefinition(name: request.toolName, description: "test", schema: request.schema)
        _ = try await model.complete(messages: [.init(role: .user, content: "story")], tools: [definition], timeout: 42)
        let calls = await transport.requests()
        XCTAssertEqual(calls[1].url.absoluteString, "https://relay.example/prefix/v1/chat/completions")
        XCTAssertEqual(calls[1].timeoutInterval, 42)
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(calls[1].body)) as? [String: Any])
        XCTAssertEqual(body["tool_choice"] as? String, "auto")
        XCTAssertEqual(body["max_tokens"] as? Int, 2_048)
        XCTAssertEqual(body["model"] as? String, "chosen-text-model")
    }

    func testAgentFactoryUsesStreamingTransportForRuntimeCalls() async throws {
        let transport = StoryPlanningTransport()
        let model = try await makeService(transport).makeAgentModel(configID: "text-model", policy: .init())
        let message = try await model.stream(messages: [.init(role: .user, content: "story")],
                                             tools: [AgentToolDefinition(name: request.toolName, description: "test", schema: request.schema)],
                                             timeout: 42, onEvent: { _ in })
        XCTAssertEqual(message.toolCalls.first?.name, "story_save_outline")
        let calls = await transport.requests()
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(calls[1].body)) as? [String: Any])
        XCTAssertEqual(body["stream"] as? Bool, true)
        XCTAssertEqual(calls[1].headers["Accept"], "text/event-stream")
    }

    func testAgentFactoryRejectsUnsupportedProviderAndOldAccountSession() async throws {
        let native = StoryPlanningTransport(scenario: "native")
        do { _ = try await makeService(native).makeAgentModel(configID: "text", policy: .init()); XCTFail("Native protocol must not be guessed") } catch {}
        let nativeCalls = await native.requests()
        XCTAssertEqual(nativeCalls.count, 1)
        let transport = StoryPlanningTransport()
        let client = ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://app.example/api/chatos")!), accessToken: "alice", transport: transport)
        let model = try await ChatOSStoryPlanningService(client: client, transport: transport).makeAgentModel(configID: "text", policy: .init())
        try await client.setAccessToken("bob")
        do { _ = try await model.complete(messages: [], tools: [], timeout: 30); XCTFail("Old account model must not be called") } catch {}
        let calls = await transport.requests()
        XCTAssertEqual(calls.count, 1)
    }

    func testUsesConfiguredModelAndAllowsOnlyTheCurrentPlanningTool() async throws {
        let transport = StoryPlanningTransport()
        let service = makeService(transport)
        let result = try await service.plan(request)
        XCTAssertEqual(String(decoding: result, as: UTF8.self), #"{"summary":"test"}"#)
        let calls = await transport.requests()
        XCTAssertEqual(calls.count, 2)
        XCTAssertEqual(calls[0].url.query, "include_secret=true")
        XCTAssertEqual(calls[1].url.absoluteString, "https://relay.example/prefix/v1/chat/completions")
        XCTAssertEqual(calls[1].headers["Authorization"], "Bearer model-secret")
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(calls[1].body)) as? [String: Any])
        XCTAssertEqual(body["model"] as? String, "chosen-text-model")
        let tools = try XCTUnwrap(body["tools"] as? [[String: Any]])
        XCTAssertEqual(tools.count, 1)
        XCTAssertEqual((tools[0]["function"] as? [String: Any])?["name"] as? String, "story_save_outline")
        XCTAssertEqual(body["stream"] as? Bool, false)
        XCTAssertFalse(String(decoding: calls[1].body!, as: UTF8.self).contains("model-secret"))
    }

    func testUnexpectedToolsAndTruncatedOutputAreRejected() async throws {
        for scenario in ["wrong-tool", "truncated", "no-tool", "multiple-tools", "invalid-json"] {
            let transport = StoryPlanningTransport(scenario: scenario)
            do {
                _ = try await makeService(transport).plan(request)
                XCTFail("Should reject \(scenario)")
            } catch {
                let calls = await transport.requests()
                XCTAssertEqual(calls.count, 2, "No automatic retry or extra tool execution")
            }
        }
    }

    func testNativeProviderIsNotSilentlySentThroughWrongProtocol() async throws {
        let transport = StoryPlanningTransport(scenario: "native")
        do { _ = try await makeService(transport).plan(request); XCTFail("Should reject unsupported protocol") }
        catch {
            let calls = await transport.requests()
            XCTAssertEqual(calls.count, 1)
        }
    }

    private var request: StoryPlanningRequest {
        .init(modelConfigID: "text-model", systemPrompt: "Plan one step", context: "story data", toolName: "story_save_outline",
              schema: Data(#"{"type":"object","properties":{"summary":{"type":"string"}},"required":["summary"]}"#.utf8))
    }
    private func makeService(_ transport: StoryPlanningTransport) -> ChatOSStoryPlanningService {
        let client = ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://app.example/api/chatos")!), accessToken: "token", transport: transport)
        return .init(client: client, transport: transport)
    }
}

private actor StoryPlanningTransport: HTTPTransport {
    private var calls: [HTTPRequest] = []
    private let scenario: String
    init(scenario: String = "valid") { self.scenario = scenario }
    func requests() -> [HTTPRequest] { calls }
    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        calls.append(request)
        let body: [String: Any]
        if request.url.path.contains("ai-model-configs") {
            body = ["enabled": true, "provider": scenario == "native" ? "anthropic" : "gpt", "model": "chosen-text-model", "base_url": "https://relay.example/prefix/v1/", "api_key": "model-secret"]
        } else {
            let tool: [String: Any] = ["id": "call_1", "type": "function", "function": ["name": scenario == "wrong-tool" ? "generate_video" : "story_save_outline", "arguments": scenario == "invalid-json" ? "not-json" : #"{"summary":"test"}"#]]
            let tools = scenario == "no-tool" ? [] : scenario == "multiple-tools" ? [tool, tool] : [tool]
            body = ["choices": [["finish_reason": scenario == "truncated" ? "length" : "tool_calls", "message": ["tool_calls": tools]]]]
        }
        return .init(statusCode: 200, headers: [:], body: try JSONSerialization.data(withJSONObject: body))
    }

    func stream(_ request: HTTPRequest) async throws -> HTTPStreamResponse {
        calls.append(request)
        let pair = AsyncThrowingStream<Data, Error>.makeStream()
        let sse = """
        data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"story_save_outline","arguments":"{\\\"summary\\\":\\\"test\\\"}"}}]},"finish_reason":"tool_calls"}]}

        data: [DONE]

        """
        pair.continuation.yield(Data(sse.utf8)); pair.continuation.finish()
        return .init(statusCode: 200, headers: ["content-type": "text/event-stream"], body: pair.stream)
    }
}
