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
                    thinkingLevel: "high"
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
}

private actor RuntimeSettingsTransport: HTTPTransport {
    private var requests: [HTTPRequest] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        let body: String
        switch (request.method, request.url.path) {
        case ("GET", "/api/chatos/ai-model-configs"):
            body = #"[{"id":"model-sol","name":"my / gpt-5.6-sol","model":"gpt-5.6-sol","thinking_level":"high","enabled":true},{"id":"model-disabled","name":"Disabled","model":"gpt-disabled","enabled":false}]"#
        case ("PUT", "/api/chatos/conversations/conversation-1/runtime-settings"):
            body = #"{"selected_model_id":"model-sol","selected_model_name":"gpt-5.6-sol","selected_thinking_level":"high","reasoning_enabled":true,"plan_mode_enabled":false}"#
        default:
            return HTTPResponse(statusCode: 404, headers: [:], body: Data())
        }
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func updateRequest() -> HTTPRequest? {
        requests.first { $0.method == "PUT" && $0.url.path.contains("runtime-settings") }
    }
}
