import Foundation
import XCTest
@testable import ChatOSAPI
@testable import ChatOSCore

final class RealtimeDTOTests: XCTestCase {
    func testInboxUpdateMapsPersistedActivityDirectly() throws {
        let data = Data(#"""
        {
          "type":"event",
          "event":"pet_activity_inbox.updated",
          "event_id":"event-inbox-1",
          "event_sequence":99,
          "conversation_id":"conversation-1",
          "project_id":"project-1",
          "payload":{
            "kind":"pet_activity_inbox_updated",
            "action":"upserted",
            "activity_id":"pet_1",
            "inbox_status":"unread",
            "activity":{
              "id":"pet_1",
              "activity_key":"task-runner:task-1",
              "activity_version":"run-2",
              "source":"task_runner",
              "kind":"succeeded",
              "title":"任务「整理发布说明」已完成",
              "route":{"task_id":"task-1","run_id":"run-2"},
              "business_status":"completed",
              "inbox_status":"unread",
              "requires_action":false,
              "occurred_at":"2026-08-28T08:00:00Z",
              "created_at":"2026-08-28T08:00:00Z",
              "updated_at":"2026-08-28T08:00:00Z"
            }
          },
          "ts":"2026-08-28T08:00:00Z"
        }
        """#.utf8)

        let envelope = try JSONDecoder().decode(PetRealtimeEnvelopeDTO.self, from: data)
        guard case let .upsert(activity) = envelope.petActivityEvent() else {
            return XCTFail("expected persisted inbox upsert")
        }
        XCTAssertEqual(activity.inboxID, "pet_1")
        XCTAssertEqual(activity.activityVersion, "run-2")
        XCTAssertEqual(activity.kind, .succeeded)
        XCTAssertNil(activity.expiresAt)
    }

    func testClosedInboxUpdateRemovesPersistedActivity() throws {
        let data = Data(#"""
        {
          "type":"event",
          "event":"pet_activity_inbox.updated",
          "event_id":"event-inbox-2",
          "event_sequence":100,
          "payload":{
            "kind":"pet_activity_inbox_updated",
            "activity":{
              "id":"pet_2",
              "activity_key":"task-runner:task-2",
              "activity_version":"run-3",
              "source":"task_runner",
              "kind":"blocked",
              "title":"任务被阻塞",
              "route":{"task_id":"task-2","run_id":"run-3"},
              "inbox_status":"handled",
              "occurred_at":"2026-08-28T08:00:00Z",
              "updated_at":"2026-08-28T08:01:00Z"
            }
          },
          "ts":"2026-08-28T08:01:00Z"
        }
        """#.utf8)

        let envelope = try JSONDecoder().decode(PetRealtimeEnvelopeDTO.self, from: data)
        XCTAssertEqual(envelope.petActivityEvent(), .remove(id: "task-runner:task-2"))
    }

    func testChatStreamEnvelopeMapsStableIdentityAndSequence() throws {
        let envelope = try JSONDecoder().decode(
            RealtimeEventEnvelopeDTO.self,
            from: Data(fixtureJSON.utf8)
        )

        let signal = try XCTUnwrap(envelope.signal(expectedSessionID: "conversation-1"))
        XCTAssertEqual(signal.eventID, "event-1")
        XCTAssertEqual(signal.eventSequence, 101)
        XCTAssertEqual(signal.turnID, "turn-1")
        XCTAssertEqual(signal.kind, .completed)
    }

    func testWebSocketURLUsesGatewayBaseAndTicket() async throws {
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!)
        )

        let generatedURL = await client.webSocketURL(path: "/realtime/ws", ticket: "ticket-1")
        let url = try XCTUnwrap(generatedURL)
        XCTAssertEqual(url.scheme, "wss")
        XCTAssertEqual(url.path, "/api/chatos/realtime/ws")
        XCTAssertEqual(url.query, "ws_ticket=ticket-1")
    }

    func testAskUserPromptEnvelopeIsDeliveredToNativeConversation() throws {
        let envelope = try JSONDecoder().decode(
            RealtimeEventEnvelopeDTO.self,
            from: Data(askUserFixtureJSON.utf8)
        )

        let signal = try XCTUnwrap(envelope.signal(expectedSessionID: "conversation-1"))
        let update = try XCTUnwrap(signal.askUserPromptUpdate)
        XCTAssertEqual(update.promptID, "prompt-1")
        XCTAssertEqual(update.turnID, "turn-1")
        XCTAssertEqual(update.action, "prompt_required")
        XCTAssertEqual(update.status, .pending)
    }

    func testAskUserPromptDoesNotBypassPetInbox() throws {
        let envelope = try JSONDecoder().decode(
            PetRealtimeEnvelopeDTO.self,
            from: Data(askUserFixtureJSON.utf8)
        )
        XCTAssertNil(envelope.petActivityEvent())
    }

    func testToolRealtimeEventProducesSafeExecutionProcessUpdate() throws {
        let envelope = try JSONDecoder().decode(
            RealtimeEventEnvelopeDTO.self,
            from: Data(toolFixtureJSON.utf8)
        )

        let signal = try XCTUnwrap(envelope.signal(expectedSessionID: "conversation-1"))
        let update = try XCTUnwrap(signal.processUpdate)
        XCTAssertEqual(update.title, "正在调用工具：create_project_execution_tasks")
        XCTAssertEqual(update.status, "running")
    }

    func testThinkingRealtimeEventDoesNotExposeRawReasoningText() throws {
        let envelope = try JSONDecoder().decode(
            RealtimeEventEnvelopeDTO.self,
            from: Data(thinkingFixtureJSON.utf8)
        )

        let signal = try XCTUnwrap(envelope.signal(expectedSessionID: "conversation-1"))
        let update = try XCTUnwrap(signal.processUpdate)
        XCTAssertEqual(update.title, "正在分析需求与任务依赖")
        XCTAssertNil(update.detail)
    }

    func testUserSubscriptionRequestsUserTopic() throws {
        let data = try XCTUnwrap(ChatOSRealtimeClient.userSubscriptionMessage().data(using: .utf8))
        let payload = try XCTUnwrap(
            JSONSerialization.jsonObject(with: data) as? [String: Any]
        )
        let topics = try XCTUnwrap(payload["topics"] as? [[String: String]])
        XCTAssertEqual(topics, [["scope": "user"]])
    }

    func testTaskRunnerEventDoesNotBypassPetInbox() throws {
        let envelope = try JSONDecoder().decode(
            PetRealtimeEnvelopeDTO.self,
            from: Data(taskRunnerFixtureJSON.utf8)
        )
        XCTAssertNil(envelope.petActivityEvent())
    }

    func testTaskBoardEventDoesNotBypassPetInbox() throws {
        let envelope = try JSONDecoder().decode(
            PetRealtimeEnvelopeDTO.self,
            from: Data(taskReviewFixtureJSON.utf8)
        )
        XCTAssertNil(envelope.petActivityEvent())
    }

    private let fixtureJSON = #"""
    {
      "type": "event",
      "event": "chat.completed",
      "event_id": "event-1",
      "event_sequence": 101,
      "user_id": "user-1",
      "conversation_id": "conversation-1",
      "project_id": "project-1",
      "payload": {
        "kind": "chat_stream",
        "conversation_id": "conversation-1",
        "conversation_turn_id": "turn-1",
        "stream_type": "completed",
        "raw": { "type": "completed" }
      },
      "ts": "2026-08-24T03:00:00Z"
    }
    """#

    private let askUserFixtureJSON = #"""
    {
      "type": "event",
      "event": "conversation.ask_user_prompt.updated",
      "event_id": "event-2",
      "event_sequence": 102,
      "conversation_id": "conversation-1",
      "payload": {
        "kind": "ask_user_prompt",
        "conversation_id": "conversation-1",
        "conversation_turn_id": "turn-1",
        "prompt_id": "prompt-1",
        "action": "prompt_required",
        "status": "pending"
      },
      "ts": "2026-08-25T08:00:00Z"
    }
    """#

    private let toolFixtureJSON = #"""
    {
      "type": "event",
      "event": "chat.tool.started",
      "event_id": "event-3",
      "event_sequence": 103,
      "conversation_id": "conversation-1",
      "payload": {
        "kind": "chat_stream",
        "conversation_id": "conversation-1",
        "conversation_turn_id": "turn-1",
        "stream_type": "tools_start",
        "raw": {
          "type": "tools_start",
          "data": {
            "tool_calls": [
              {"function": {"name": "create_project_execution_tasks"}}
            ]
          }
        }
      },
      "ts": "2026-08-25T08:01:00Z"
    }
    """#

    private let thinkingFixtureJSON = #"""
    {
      "type": "event",
      "event": "chat.turn.thinking",
      "event_id": "event-4",
      "event_sequence": 104,
      "conversation_id": "conversation-1",
      "payload": {
        "kind": "chat_stream",
        "conversation_id": "conversation-1",
        "conversation_turn_id": "turn-1",
        "stream_type": "thinking",
        "raw": {
          "type": "thinking",
          "content": "private chain of thought"
        }
      },
      "ts": "2026-08-25T08:02:00Z"
    }
    """#

    private let taskRunnerFixtureJSON = #"""
    {
      "type": "event",
      "event": "chat.task_runner.updated",
      "event_id": "event-5",
      "event_sequence": 105,
      "conversation_id": "conversation-1",
      "project_id": "project-1",
      "payload": {
        "kind": "chat_stream",
        "conversation_id": "conversation-1",
        "conversation_turn_id": "turn-1",
        "project_id": "project-1",
        "stream_type": "task_runner_callback",
        "raw": {
          "type": "task_runner_callback",
          "event": "task.completed",
          "result": {
            "persisted_user_message": {
              "metadata": {
                "task_runner_async": {
                  "last_task_id": "task-1",
                  "last_run_id": "run-1"
                }
              }
            },
            "persisted_assistant_message": {"content": "任务结果已保存"}
          }
        }
      },
      "ts": "2026-08-28T08:00:00Z"
    }
    """#

    private let taskReviewFixtureJSON = #"""
    {
      "type": "event",
      "event": "conversation.task_board.updated",
      "event_id": "event-6",
      "event_sequence": 106,
      "conversation_id": "conversation-1",
      "payload": {
        "kind": "task_board",
        "conversation_id": "conversation-1",
        "conversation_turn_id": "turn-1",
        "review_id": "review-1",
        "action": "review_required"
      },
      "ts": "2026-08-28T08:01:00Z"
    }
    """#
}
