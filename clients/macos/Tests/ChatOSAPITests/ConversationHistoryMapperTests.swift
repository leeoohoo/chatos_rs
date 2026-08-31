import Foundation
import XCTest
@testable import ChatOSAPI
@testable import ChatOSCore

final class ConversationHistoryMapperTests: XCTestCase {
    func testCompactHistoryMapsStableTurnAndFinalAssistant() throws {
        let response = try JSONDecoder().decode(
            CompactHistoryResponseDTO.self,
            from: Data(fixtureJSON.utf8)
        )

        let page = ConversationHistoryMapper.map(
            response,
            sessionID: "conversation-1",
            requestGeneration: 7
        )

        XCTAssertEqual(page.turns.count, 1)
        XCTAssertEqual(page.turns[0].id, "turn-1")
        XCTAssertEqual(page.turns[0].userMessage.text, "请检查聊天历史")
        XCTAssertEqual(page.turns[0].finalAssistantMessage?.text, "已经开始检查。")
        XCTAssertEqual(page.turns[0].assistantReplies.map(\.message.text), ["已经开始检查。"])
        XCTAssertEqual(page.turns[0].processEvents.count, 1)
        XCTAssertEqual(page.turns[0].status, .completed)
        XCTAssertEqual(page.turns[0].projectExecutionContext?.projectID, "project-1")
        XCTAssertEqual(page.turns[0].projectExecutionContext?.executionGroupID, "group-1")
        XCTAssertEqual(page.turns[0].messageTaskLookup?.sourceUserMessageID, "group-1")
        XCTAssertEqual(page.turns[0].messageTaskLookup?.turnID, "turn-1")
        XCTAssertEqual(page.turns[0].revision, 6)
        XCTAssertEqual(page.olderCursor, "turn-0")
        XCTAssertTrue(page.hasOlder)
        XCTAssertEqual(page.requestGeneration, 7)
        XCTAssertEqual(page.snapshotRevision, 6)
    }

    func testCancelledTaskCallbackDoesNotReplaceVisibleAssistantReply() throws {
        let response = try JSONDecoder().decode(
            CompactHistoryResponseDTO.self,
            from: Data(cancelledCallbackFixtureJSON.utf8)
        )

        let page = ConversationHistoryMapper.map(
            response,
            sessionID: "conversation-1",
            requestGeneration: 1
        )

        XCTAssertEqual(page.turns.count, 1)
        XCTAssertEqual(page.turns[0].finalAssistantMessage?.text, "任务执行失败，请检查运行详情。")
        XCTAssertFalse(page.turns[0].finalAssistantMessage?.text.contains("已取消") == true)
        XCTAssertEqual(page.turns[0].assistantReplies.map(\.message.id), ["assistant-failed"])
    }

    func testTaskGraphEntryDoesNotCreateFakeConversationProcess() throws {
        let response = try JSONDecoder().decode(
            CompactHistoryResponseDTO.self,
            from: Data(taskWithoutProcessFixtureJSON.utf8)
        )

        let page = ConversationHistoryMapper.map(
            response,
            sessionID: "conversation-1",
            requestGeneration: 1
        )

        XCTAssertTrue(page.turns[0].processEvents.isEmpty)
        XCTAssertEqual(page.turns[0].messageTaskLookup?.sourceUserMessageID, "group-1")
    }

    func testStoppedTaskBatchStillKeepsHistoricalTurnAndTaskLookup() throws {
        let json = taskWithoutProcessFixtureJSON.replacingOccurrences(
            of: #""confirmation_status": "confirmed""#,
            with: #""confirmation_status": "stopped""#
        )
        let response = try JSONDecoder().decode(
            CompactHistoryResponseDTO.self,
            from: Data(json.utf8)
        )

        let page = ConversationHistoryMapper.map(
            response,
            sessionID: "conversation-1",
            requestGeneration: 1
        )

        XCTAssertEqual(page.turns.count, 1)
        XCTAssertEqual(page.turns[0].userMessage.text, "执行任务")
        XCTAssertTrue(page.turns[0].processEvents.isEmpty)
        XCTAssertEqual(page.turns[0].messageTaskLookup?.sourceUserMessageID, "group-1")
    }

    func testOneUserMessageKeepsEveryTaskRunnerAgentReply() throws {
        let response = try JSONDecoder().decode(
            CompactHistoryResponseDTO.self,
            from: Data(multipleAgentRepliesFixtureJSON.utf8)
        )

        let page = ConversationHistoryMapper.map(
            response,
            sessionID: "conversation-1",
            requestGeneration: 1
        )

        XCTAssertEqual(page.turns.count, 1)
        let replies = page.turns[0].assistantReplies
        XCTAssertEqual(
            replies.map(\.message.id),
            ["assistant-summary", "callback-started", "callback-completed"]
        )
        XCTAssertNil(replies[0].taskCallback)
        XCTAssertEqual(replies[1].taskCallback?.taskID, "task-1")
        XCTAssertEqual(replies[1].taskCallback?.runID, "run-1")
        XCTAssertEqual(replies[1].taskCallback?.event, "task.started")
        XCTAssertEqual(replies[2].taskCallback?.event, "task.completed")
    }

    func testTerminalCallbackEventOverridesStaleRunningStatus() throws {
        let response = try JSONDecoder().decode(
            CompactHistoryResponseDTO.self,
            from: Data(#"{"items":[{"id":"user-1","conversation_id":"conversation-1","role":"user","content":"执行任务","metadata":{"conversation_turn_id":"turn-1"}},{"id":"callback-1","conversation_id":"conversation-1","role":"assistant","content":"任务已完成","message_mode":"task_runner_callback","metadata":{"task_runner_async":{"message_kind":"task_terminal_update","event":"task.completed","status":"running","task_id":"task-1","source_turn_id":"turn-1","source_user_message_id":"user-1"}}}],"has_more":false}"#.utf8)
        )

        let page = ConversationHistoryMapper.map(
            response,
            sessionID: "conversation-1",
            requestGeneration: 1
        )

        let callback = try XCTUnwrap(page.turns.first?.assistantReplies.first?.taskCallback)
        XCTAssertEqual(callback.event, "task.completed")
        XCTAssertEqual(callback.status, "completed")
    }

    func testUserMessageMapsPersistedAttachments() throws {
        let response = try JSONDecoder().decode(
            CompactHistoryResponseDTO.self,
            from: Data(#"{"items":[{"id":"user-with-file","role":"user","content":"请看附件","metadata":{"conversation_turn_id":"turn-file","attachments":[{"id":"attachment-1","name":"design.pdf","mimeType":"application/pdf","size":2048,"type":"file","storageProvider":"minio","bucket":"attachments","objectKey":"design.pdf","viewUrl":"/api/attachments/object?token=abc"}]}}],"has_more":false}"#.utf8)
        )

        let page = ConversationHistoryMapper.map(
            response,
            sessionID: "conversation-1",
            requestGeneration: 1
        )

        let attachment = try XCTUnwrap(page.turns.first?.userMessage.attachments.first)
        XCTAssertEqual(attachment.name, "design.pdf")
        XCTAssertEqual(attachment.mimeType, "application/pdf")
        XCTAssertEqual(attachment.size, 2048)
        XCTAssertEqual(attachment.kind, .file)
        XCTAssertEqual(attachment.objectKey, "design.pdf")
    }

    func testTerminalTaskMetadataDoesNotLeaveTurnStreaming() throws {
        let response = try JSONDecoder().decode(
            CompactHistoryResponseDTO.self,
            from: Data(#"{"items":[{"id":"user-failed","role":"user","content":"你好","metadata":{"conversation_turn_id":"turn-failed","task_runner_async":{"overall_status":"failed","confirmation_status":"failed"}}}],"has_more":false}"#.utf8)
        )

        let page = ConversationHistoryMapper.map(
            response,
            sessionID: "conversation-1",
            requestGeneration: 1
        )

        XCTAssertEqual(page.turns.first?.status, .failed)
    }

    private let fixtureJSON = #"""
    {
      "items": [
        {
          "id": "user-1",
          "conversation_id": "conversation-1",
          "sequence_no": 9,
          "revision": 4,
          "role": "user",
          "content": "请检查聊天历史",
          "created_at": "2026-08-24T03:00:00.000Z",
          "updated_at": "2026-08-24T03:00:01.000Z",
          "metadata": {
            "conversation_turn_id": "turn-1",
            "project_requirement_execution": {
              "project_id": "project-1",
              "requirement_id": "requirement-1",
              "execution_group_id": "group-1",
              "contact_id": "contact-1"
            },
            "task_runner_async": {
              "mode": "project_requirement_execution",
              "source_user_message_id": "group-1",
              "source_turn_id": "turn-1",
              "confirmation_status": "awaiting_confirmation",
              "overall_status": "awaiting_confirmation"
            },
            "historyProcess": {
              "turnId": "turn-1",
              "finalAssistantMessageId": "assistant-1",
              "processMessageCount": 3
            }
          }
        },
        {
          "id": "assistant-1",
          "conversation_id": "conversation-1",
          "revision": 6,
          "role": "assistant",
          "content": "已经开始检查。",
          "status": "completed",
          "created_at": "2026-08-24T03:00:05Z",
          "updated_at": "2026-08-24T03:00:06Z",
          "metadata": {
            "historyFinalForUserMessageId": "user-1",
            "historyFinalForTurnId": "turn-1"
          }
        }
      ],
      "has_more": true,
      "next_before": "turn-0",
      "snapshot_revision": 6
    }
    """#

    private let cancelledCallbackFixtureJSON = #"""
    {
      "items": [
        {
          "id": "user-1",
          "conversation_id": "conversation-1",
          "role": "user",
          "content": "执行任务",
          "created_at": "2026-08-24T03:00:00Z",
          "metadata": {"conversation_turn_id": "turn-1"}
        },
        {
          "id": "assistant-failed",
          "conversation_id": "conversation-1",
          "role": "assistant",
          "content": "任务执行失败，请检查运行详情。",
          "created_at": "2026-08-24T03:00:01Z",
          "metadata": {"historyFinalForTurnId": "turn-1"}
        },
        {
          "id": "callback-cancelled",
          "conversation_id": "conversation-1",
          "role": "assistant",
          "content": "任务「编写 README」已取消",
          "message_mode": "task_runner_callback",
          "created_at": "2026-08-24T03:00:02Z",
          "metadata": {
            "historyFinalForTurnId": "turn-1",
            "task_runner_async": {
              "message_kind": "task_terminal_update",
              "event": "task.cancelled",
              "status": "cancelled"
            }
          }
        }
      ],
      "has_more": false
    }
    """#

    private let taskWithoutProcessFixtureJSON = #"""
    {
      "items": [
        {
          "id": "user-1",
          "conversation_id": "conversation-1",
          "role": "user",
          "content": "执行任务",
          "created_at": "2026-08-24T03:00:00Z",
          "metadata": {
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
              "mode": "contact_async",
              "source_turn_id": "turn-1",
              "source_user_message_id": "group-1",
              "confirmation_status": "confirmed"
            },
            "historyProcess": {"processMessageCount": 0}
          }
        }
      ],
      "has_more": false
    }
    """#

    private let multipleAgentRepliesFixtureJSON = #"""
    {
      "items": [
        {
          "id": "user-1",
          "conversation_id": "conversation-1",
          "role": "user",
          "content": "执行这个任务",
          "created_at": "2026-08-24T03:00:00Z",
          "metadata": {
            "conversation_turn_id": "turn-1",
            "historyProcess": {
              "turnId": "turn-1",
              "finalAssistantMessageId": "assistant-summary",
              "processMessageCount": 4
            },
            "task_runner_async": {
              "source_turn_id": "turn-1",
              "source_user_message_id": "user-1"
            }
          }
        },
        {
          "id": "assistant-summary",
          "conversation_id": "conversation-1",
          "role": "assistant",
          "content": "任务已经开始。",
          "created_at": "2026-08-24T03:00:01Z",
          "metadata": {
            "historyFinalForUserMessageId": "user-1",
            "historyFinalForTurnId": "turn-1"
          }
        },
        {
          "id": "callback-started",
          "conversation_id": "conversation-1",
          "role": "assistant",
          "content": "任务节点开始执行",
          "message_mode": "task_runner_callback",
          "created_at": "2026-08-24T03:00:02Z",
          "metadata": {
            "task_runner_async": {
              "message_kind": "task_lifecycle_update",
              "event": "task.started",
              "status": "running",
              "task_id": "task-1",
              "run_id": "run-1",
              "source_session_id": "conversation-1",
              "source_turn_id": "turn-1",
              "source_user_message_id": "user-1"
            }
          }
        },
        {
          "id": "callback-completed",
          "conversation_id": "conversation-1",
          "role": "assistant",
          "content": "任务节点执行完成",
          "message_mode": "task_runner_callback",
          "created_at": "2026-08-24T03:00:03Z",
          "metadata": {
            "task_runner_async": {
              "message_kind": "task_terminal_update",
              "event": "task.completed",
              "status": "succeeded",
              "task_id": "task-1",
              "run_id": "run-1",
              "source_session_id": "conversation-1",
              "source_turn_id": "turn-1",
              "source_user_message_id": "user-1"
            }
          }
        }
      ],
      "has_more": false
    }
    """#
}
