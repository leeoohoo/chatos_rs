import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSAskUserPromptServiceTests: XCTestCase {
    func testFetchMapsFieldsSecretsAndChoiceRules() async throws {
        let transport = AskUserPromptTransport()
        let service = ChatOSAskUserPromptService(client: ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        ))

        let prompts = try await service.fetchPrompts(sessionID: "conversation-1", limit: 20)

        let prompt = try XCTUnwrap(prompts.first)
        XCTAssertEqual(prompt.turnID, "turn-1")
        XCTAssertEqual(prompt.title, "部署设置")
        XCTAssertTrue(prompt.fields[0].isSecret)
        XCTAssertTrue(prompt.fields[0].isRequired)
        XCTAssertEqual(prompt.choice?.options.map(\.value), ["staging", "production"])
        XCTAssertEqual(prompt.choice?.defaultSelection, ["staging"])
        XCTAssertEqual(prompt.choice?.minimumSelectionCount, 1)

        let requests = await transport.requests
        let request = try XCTUnwrap(requests.first)
        XCTAssertEqual(request.path, "/api/chatos/ask-user-prompts")
        XCTAssertEqual(
            request.query,
            "conversation_id=conversation-1&include_pending=true&limit=20"
        )
    }

    func testSubmitUsesScalarForSingleChoiceAndKeepsFieldValues() async throws {
        let transport = AskUserPromptTransport()
        let service = ChatOSAskUserPromptService(client: ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        ))

        _ = try await service.submit(
            promptID: "prompt/1",
            sessionID: "conversation-1",
            submission: AskUserSubmission(
                values: ["token": "top-secret"],
                selection: .single("production")
            )
        )

        let requests = await transport.requests
        let request = try XCTUnwrap(requests.last)
        XCTAssertEqual(request.path, "/api/chatos/ask-user-prompts/prompt%2F1/submit")
        let object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: request.body ?? Data()) as? [String: Any]
        )
        XCTAssertEqual(object["conversation_id"] as? String, "conversation-1")
        XCTAssertEqual((object["values"] as? [String: String])?["token"], "top-secret")
        XCTAssertEqual(object["selection"] as? String, "production")
    }
}

private actor AskUserPromptTransport: HTTPTransport {
    struct Request: Sendable {
        var path: String
        var query: String?
        var body: Data?
    }

    private(set) var requests: [Request] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(Request(
            path: request.url.path(percentEncoded: true),
            query: request.url.query(percentEncoded: true),
            body: request.body
        ))
        let resolved = request.method == "POST"
        let record = Self.record(status: resolved ? "ok" : "pending")
        let body = request.method == "GET"
            ? "{\"success\":true,\"prompts\":[\(record)]}"
            : "{\"success\":true,\"prompt\":\(record)}"
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    private static func record(status: String) -> String {
        #"""
        {
          "id":"prompt/1",
          "conversation_id":"conversation-1",
          "conversation_turn_id":"turn-1",
          "tool_call_id":"tool-1",
          "kind":"mixed",
          "status":"\#(status)",
          "prompt":{
            "title":"部署设置",
            "message":"请选择环境并提供访问令牌。",
            "allow_cancel":true,
            "timeout_ms":86400000,
            "payload":{
              "fields":[{
                "key":"token",
                "label":"访问令牌",
                "required":true,
                "secret":true,
                "placeholder":"请输入令牌"
              }],
              "choice":{
                "multiple":false,
                "options":[
                  {"value":"staging","label":"预发布"},
                  {"value":"production","label":"生产环境"}
                ],
                "default":"staging",
                "min_selections":1,
                "max_selections":1
              }
            }
          },
          "created_at":"2026-08-25T08:00:00Z",
          "updated_at":"2026-08-25T08:00:00Z"
        }
        """#
    }
}
