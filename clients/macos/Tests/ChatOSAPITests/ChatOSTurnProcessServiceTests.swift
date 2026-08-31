import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSTurnProcessServiceTests: XCTestCase {
    func testTurnProcessLoadsOnlyRealTaskAndToolNodes() async throws {
        let transport = TurnProcessTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        let nodes = try await ChatOSTurnProcessService(client: client).fetchProcessNodes(
            sessionID: "conversation-1",
            turnID: "turn-1"
        )

        XCTAssertEqual(nodes.count, 2)
        XCTAssertEqual(nodes[0].title, "调用工具 · shell")
        XCTAssertEqual(nodes[0].kind, .tool)
        XCTAssertEqual(nodes[1].title, "任务 · Task A")
        XCTAssertEqual(nodes[1].detail, "Read files")
        XCTAssertEqual(nodes[1].status, .queued)
        let requestPath = await transport.requestPath()
        XCTAssertEqual(
            requestPath,
            "/api/chatos/conversations/conversation-1/turns/by-turn/turn-1/messages"
        )
    }
}

private actor TurnProcessTransport: HTTPTransport {
    private var path: String?

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        path = request.url.path
        let body = #"""
        [
          {"id":"user-1","role":"user","content":"do it","turn_id":"turn-1","metadata":{},"created_at":"2026-08-24T05:00:00Z"},
          {"id":"assistant-tool","role":"assistant","content":"准备执行","turn_id":"turn-1","status":"streaming","tool_calls":[{"function":{"name":"shell"}}],"metadata":{"historyProcessLoaded":true},"created_at":"2026-08-24T05:00:01Z"},
          {"id":"tool-1","role":"tool","content":"{\"_structured_result\":{\"title\":\"Task A\",\"status\":\"queued\",\"description\":\"Read files\"}}","turn_id":"turn-1","status":"completed","metadata":{"tool_name":"shell","historyProcessLoaded":true},"created_at":"2026-08-24T05:00:02Z"},
          {"id":"callback-1","role":"assistant","content":"task done","message_mode":"task_runner_callback","metadata":{"task_runner_async":{"message_kind":"task_terminal_update","event":"task.completed","task_id":"task-1","run_id":"run-1","source_turn_id":"turn-1","source_user_message_id":"user-1","status":"succeeded"}},"created_at":"2026-08-24T05:00:02Z"},
          {"id":"assistant-final","role":"assistant","content":"done","turn_id":"turn-1","status":"completed","metadata":{"historyFinalForUserMessageId":"user-1"},"created_at":"2026-08-24T05:00:03Z"}
        ]
        """#
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func requestPath() -> String? { path }
}
