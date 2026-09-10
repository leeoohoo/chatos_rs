import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSConversationRuntimeSettingsServiceTests: XCTestCase {
    func testAvailableModelsComeFromAuthoritativeCatalog() async throws {
        let transport = RuntimeSettingsTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        let models = try await ChatOSConversationRuntimeSettingsService(client: client)
            .fetchAvailableModels()

        XCTAssertEqual(
            models,
            [
                ConversationModelOption(
                    id: "model-sol",
                    displayName: "my / gpt-5.6-sol",
                    modelName: "gpt-5.6-sol",
                    provider: "gpt",
                    thinkingLevel: "high",
                    supportsReasoning: true,
                    thinkingLevels: ["none", "minimal", "low", "medium", "high", "xhigh"]
                ),
            ]
        )
    }

    func testUpdatingModelSendsOnlyAuthoritativeModelID() async throws {
        let transport = RuntimeSettingsTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        let settings = try await ChatOSConversationRuntimeSettingsService(client: client)
            .updateModel(sessionID: "conversation-1", modelID: "model-sol")

        XCTAssertEqual(settings.selectedModelID, "model-sol")
        XCTAssertEqual(settings.selectedModelName, "gpt-5.6-sol")
        let capturedRequest = await transport.updateRequest()
        let request = try XCTUnwrap(capturedRequest)
        let body = try XCTUnwrap(request.body)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(payload["selected_model_id"] as? String, "model-sol")
        XCTAssertNil(payload["selected_model_name"])
    }

    func testBindsAndUnbindsRemoteConnection() async throws {
        let transport = RuntimeSettingsTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )
        let service = ChatOSConversationRuntimeSettingsService(client: client)

        let bound = try await service.updateRemoteConnection(
            sessionID: "conversation-1",
            connectionID: "remote-aliyun"
        )
        let unbound = try await service.updateRemoteConnection(
            sessionID: "conversation-1",
            connectionID: nil
        )

        XCTAssertEqual(bound.remoteConnectionID, "remote-aliyun")
        XCTAssertNil(unbound.remoteConnectionID)
        let requests = await transport.updateRequests()
        XCTAssertEqual(requests.count, 2)
        let boundPayload = try payload(for: requests[0])
        let unboundPayload = try payload(for: requests[1])
        XCTAssertEqual(boundPayload["remote_connection_id"] as? String, "remote-aliyun")
        XCTAssertTrue(unboundPayload["remote_connection_id"] is NSNull)
    }

    func testUpdatingReasoningLevelSendsLevelAndEnabledAtomically() async throws {
        let transport = RuntimeSettingsTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )
        let service = ChatOSConversationRuntimeSettingsService(client: client)

        let enabled = try await service.updateReasoningLevel(
            sessionID: "conversation-1",
            level: "xhigh",
            enabled: true
        )
        let disabled = try await service.updateReasoningLevel(
            sessionID: "conversation-1",
            level: "none",
            enabled: false
        )

        XCTAssertEqual(enabled.selectedThinkingLevel, "xhigh")
        XCTAssertTrue(enabled.reasoningEnabled)
        XCTAssertEqual(disabled.selectedThinkingLevel, "none")
        XCTAssertFalse(disabled.reasoningEnabled)

        let requests = await transport.updateRequests()
        let enabledPayload = try payload(for: requests[0])
        let disabledPayload = try payload(for: requests[1])
        XCTAssertEqual(enabledPayload["selected_thinking_level"] as? String, "xhigh")
        XCTAssertEqual(enabledPayload["reasoning_enabled"] as? Bool, true)
        XCTAssertEqual(disabledPayload["selected_thinking_level"] as? String, "none")
        XCTAssertEqual(disabledPayload["reasoning_enabled"] as? Bool, false)
    }

    private func payload(for request: HTTPRequest) throws -> [String: Any] {
        let body = try XCTUnwrap(request.body)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
    }
}

private actor RuntimeSettingsTransport: HTTPTransport {
    private var requests: [HTTPRequest] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        let body: String
        switch (request.method, request.url.path) {
        case ("GET", "/api/chatos/ai-model-configs"):
            body = #"[{"id":"model-sol","name":"my / gpt-5.6-sol","provider":"gpt","model":"gpt-5.6-sol","thinking_level":"high","thinking_levels":["none","minimal","low","medium","high","xhigh"],"supports_reasoning":true,"enabled":true,"task_enabled":false},{"id":"model-sol","name":"my / gpt-5.6-sol","provider":"gpt","model":"gpt-5.6-sol","thinking_level":"high","thinking_levels":["none","minimal","low","medium","high","xhigh"],"supports_reasoning":true,"enabled":true},{"id":"stale-duplicate","name":"my / gpt-5.6-sol","provider":"gpt","model":"gpt-5.6-sol","thinking_level":"high","thinking_levels":["none","minimal","low","medium","high","xhigh"],"supports_reasoning":true,"enabled":true},{"id":"model-disabled","name":"Disabled","model":"gpt-disabled","enabled":false}]"#
        case ("PUT", "/api/chatos/conversations/conversation-1/runtime-settings"):
            let payload = request.body
                .flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
                ?? [:]
            if let connectionID = payload["remote_connection_id"] as? String {
                body = #"{"remote_connection_id":"\#(connectionID)","reasoning_enabled":true}"#
            } else if payload["remote_connection_id"] is NSNull {
                body = #"{"remote_connection_id":null,"reasoning_enabled":true}"#
            } else if let level = payload["selected_thinking_level"] as? String,
                      let enabled = payload["reasoning_enabled"] as? Bool {
                body = #"{"selected_thinking_level":"\#(level)","reasoning_enabled":\#(enabled)}"#
            } else {
                body = #"{"selected_model_id":"model-sol","selected_model_name":"gpt-5.6-sol","selected_thinking_level":"high","reasoning_enabled":true}"#
            }
        default:
            return HTTPResponse(statusCode: 404, headers: [:], body: Data())
        }
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func updateRequest() -> HTTPRequest? {
        requests.first { $0.method == "PUT" && $0.url.path.contains("runtime-settings") }
    }

    func updateRequests() -> [HTTPRequest] {
        requests.filter { $0.method == "PUT" && $0.url.path.contains("runtime-settings") }
    }
}
