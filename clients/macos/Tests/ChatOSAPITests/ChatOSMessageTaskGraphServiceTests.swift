import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSMessageTaskGraphServiceTests: XCTestCase {
    func testLoadsMessageGraphWithSourceLookupAndMapsMultipleNodes() async throws {
        let transport = MessageTaskGraphTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )
        let graph = try await ChatOSMessageTaskGraphService(client: client).fetchGraph(
            messageID: "message-1",
            lookup: MessageTaskLookup(
                sessionID: "session-1",
                turnID: "turn-1",
                sourceUserMessageID: "message-1"
            )
        )

        XCTAssertEqual(graph.nodes.count, 2)
        XCTAssertEqual(graph.edges.count, 1)
        XCTAssertEqual(graph.nodes[1].task.prerequisiteTaskIDs, ["task-1"])
        let capturedRequest = await transport.lastRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertEqual(request.url.path, "/api/chatos/messages/message-1/task-runner/graph")
        XCTAssertTrue(request.url.query?.contains("turn_id=turn-1") == true)
    }

    func testRunEventsUseRequestedPageInsteadOfLoadingEverything() async throws {
        let transport = MessageTaskGraphTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        let detail = try await ChatOSMessageTaskGraphService(client: client).fetchRun(
            messageID: "message-1",
            runID: "run-2",
            lookup: MessageTaskLookup(sessionID: "session-1"),
            includeEvents: true,
            eventLimit: 40,
            eventOffset: 80
        )

        XCTAssertEqual(detail.eventsTotal, 121)
        XCTAssertTrue(detail.eventsHasMore)
        XCTAssertEqual(detail.run.reportContent, "模型生成的主要结果")
        let capturedRequest = await transport.lastRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertTrue(request.url.query?.contains("event_limit=40") == true)
        XCTAssertTrue(request.url.query?.contains("event_offset=80") == true)
        XCTAssertTrue(request.url.query?.contains("include_events=true") == true)
    }

    func testRunSummaryCanSkipEventsWhenOnlyLoadingModelOutput() async throws {
        let transport = MessageTaskGraphTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        _ = try await ChatOSMessageTaskGraphService(client: client).fetchRun(
            messageID: "message-1",
            runID: "run-2",
            lookup: nil,
            includeEvents: false,
            eventLimit: 1,
            eventOffset: 0
        )

        let capturedRequest = await transport.lastRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertTrue(request.url.query?.contains("include_events=false") == true)
        XCTAssertTrue(request.url.query?.contains("event_limit=1") == true)
    }

    func testCancelTaskUsesScopedMessageEndpoint() async throws {
        let transport = MessageTaskGraphTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        try await ChatOSMessageTaskGraphService(client: client).cancelTask(
            messageID: "message-1",
            taskID: "task-2",
            lookup: MessageTaskLookup(sessionID: "session-1", turnID: "turn-1"),
            reason: "用户从宠物面板取消"
        )

        let capturedRequest = await transport.lastRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertEqual(request.method, "POST")
        XCTAssertEqual(
            request.url.path,
            "/api/chatos/messages/message-1/task-runner/tasks/task-2/cancel"
        )
        XCTAssertTrue(request.url.query?.contains("session_id=session-1") == true)
        let body = try XCTUnwrap(request.body)
        let payload = try XCTUnwrap(
            JSONSerialization.jsonObject(with: body) as? [String: String]
        )
        XCTAssertEqual(payload["reason"], "用户从宠物面板取消")
    }

    func testCompleteTaskDetailMapsFieldsUsedByNativeInspector() throws {
        let body = #"""
        {
          "id":"task-2",
          "title":"实现客户端",
          "description":"使用 SwiftUI",
          "objective":"完成原生重写",
          "status":"completed",
          "priority":2,
          "tags":["swift","client"],
          "creator_user_id":"user-1",
          "creator_display_name":"Lee",
          "default_model_config_id":"model-config-1",
          "default_model_config":{"id":"model-config-1","name":"主模型","provider":"openai","model":"gpt-5"},
          "last_run_id":"run-2",
          "last_run":{"id":"run-2","status":"completed","report":{"content":"完整模型输出"}},
          "result_summary":"执行摘要",
          "parent_task_id":"task-parent",
          "parent_task":{"id":"task-parent","title":"父任务","status":"completed"},
          "prerequisite_task_ids":["task-1"],
          "prerequisite_tasks":[{"id":"task-1","title":"前置节点","status":"completed"}],
          "source_session_id":"session-1",
          "source_turn_id":"turn-1",
          "source_user_message_id":"message-1",
          "mcp_config":{"workspace_dir":"/tmp/project"},
          "task_tool_state":{"outcome_items":["a"]},
          "schedule":{"mode":"immediate"},
          "input_payload":{"project_task_id":"project-task-1","execution_client_ref":"gateway-client"}
        }
        """#

        let dto = try JSONDecoder().decode(MessageTaskDTO.self, from: Data(body.utf8))
        let task = dto.model

        XCTAssertEqual(task.lastRun?.reportContent, "完整模型输出")
        XCTAssertEqual(task.defaultModelConfig?.displayName, "主模型 · openai/gpt-5")
        XCTAssertEqual(task.creatorDisplayName, "Lee")
        XCTAssertEqual(task.prerequisiteTasks.first?.title, "前置节点")
        XCTAssertEqual(task.projectTaskID, "project-task-1")
        XCTAssertEqual(task.executionClientRef, "gateway-client")
        XCTAssertTrue(task.mcpConfigJSON?.contains("workspace_dir") == true)
        XCTAssertTrue(task.taskToolStateJSON?.contains("outcome_items") == true)
    }
}

private actor MessageTaskGraphTransport: HTTPTransport {
    private var request: HTTPRequest?

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        self.request = request
        if request.url.path.hasSuffix("/cancel") {
            return HTTPResponse(
                statusCode: 200,
                headers: [:],
                body: Data(#"{"success":true}"#.utf8)
            )
        }
        if request.url.path.contains("/runs/") {
            let body = #"""
            {
              "task":{"id":"task-2","title":"任务二","status":"running","last_run_id":"run-2","prerequisite_task_ids":[]},
              "run":{"id":"run-2","task_id":"task-2","status":"running","report":{"content":"模型生成的主要结果"}},
              "events":[{"id":"event-81","event_type":"thinking","message":"继续处理"}],
              "events_total":121,
              "events_has_more":true
            }
            """#
            return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
        }
        let body = #"""
        {
          "root_task_ids":["task-2"],
          "source_session_id":"session-1",
          "source_turn_id":"turn-1",
          "source_user_message_id":"message-1",
          "nodes":[
            {"depth":0,"is_root":false,"is_current_message":true,"task":{"id":"task-1","title":"任务一","status":"completed","prerequisite_task_ids":[]}},
            {"depth":1,"is_root":true,"is_current_message":true,"task":{"id":"task-2","title":"任务二","status":"blocked","last_run_id":"run-2","prerequisite_task_ids":["task-1"]}}
          ],
          "edges":[{"id":"task-1->task-2","source":"task-1","target":"task-2","kind":"prerequisite"}]
        }
        """#
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func lastRequest() -> HTTPRequest? { request }
}
