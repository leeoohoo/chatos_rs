@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class LocalAgentTurnProcessMapperTests: XCTestCase {
    func testNewUserTurnCompletesPendingTaskGraphRecoveryNode() throws {
        let data = try JSONSerialization.data(withJSONObject: [[
            "cursor": 1, "event_id": "event-1", "run_id": "run-1",
            "event_type": "task_graph_recovery_superseded", "created_at_unix_ms": 1,
            "payload": ["newer_turn_id": "turn-2"],
        ]])
        let events = try JSONDecoder().decode([LocalAgentEventRecord].self, from: data)
        let nodes = LocalAgentTurnProcessMapper.map(events)

        XCTAssertEqual(nodes.first?.status, .completed)
        XCTAssertEqual(nodes.first?.title, "已按你的新消息停止旧的自动协调")
    }

    func testReviewNodeShowsUnderlyingFailureAndKeepsSensitiveDetailsHidden() throws {
        for (error, expected) in [
            ("Memory runtime context assertion issuer returned 401 Unauthorized",
             "Memory runtime context assertion issuer returned 401 Unauthorized"),
            ("request failed: Authorization: Bearer private-value", "详细信息已隐藏"),
            ("  ", "Local Agent profile step failed"),
        ] {
            let data = try JSONSerialization.data(withJSONObject: [[
                "cursor": 1, "event_id": "event-1", "run_id": "run-1",
                "event_type": "run_needs_review", "created_at_unix_ms": 1,
                "payload": ["reason": "Local Agent profile step failed", "detail": ["error": error]],
            ]])
            let events = try JSONDecoder().decode([LocalAgentEventRecord].self, from: data)
            let nodes = LocalAgentTurnProcessMapper.map(events)
            XCTAssertEqual(nodes.count, 1)
            XCTAssertEqual(nodes.first?.status, .failed)
            XCTAssertEqual(nodes.first?.detail, expected)
        }
    }

    func testFailedToolCallShowsSafeUnderlyingError() throws {
        for (error, expected) in [
            ("upstream stream ended without a terminal event", "upstream stream ended without a terminal event"),
            ("Authorization: Bearer private-value", "详细信息已隐藏"),
        ] {
            let data = try JSONSerialization.data(withJSONObject: [
                [
                    "cursor": 1, "event_id": "event-1", "run_id": "run-1",
                    "event_type": "tool_batch_requested", "created_at_unix_ms": 1,
                    "payload": ["tool_calls": [[
                        "call_id": "call-1", "tool_name": "stage_edit_batch",
                    ]]],
                ],
                [
                    "cursor": 2, "event_id": "event-2", "run_id": "run-1",
                    "event_type": "tool_invocation_completed", "created_at_unix_ms": 2,
                    "payload": ["call_id": "call-1", "status": "failed", "error": error],
                ],
            ])
            let events = try JSONDecoder().decode([LocalAgentEventRecord].self, from: data)
            let nodes = LocalAgentTurnProcessMapper.map(events)

            XCTAssertEqual(nodes.count, 1)
            XCTAssertEqual(nodes.first?.status, .failed)
            XCTAssertEqual(nodes.first?.detail, expected)
        }
    }
}
