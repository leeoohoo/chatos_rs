@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentRunActivityProjectorTests: XCTestCase {
    func testProjectsDistinctWaitReasonsAndDetails() throws {
        let largeResult = try run(
            status: "waiting_tool_result",
            extra: [
                "pending_tool_batch": [
                    "batch_id": "batch-1",
                    "calls": [[
                        "call_id": "call-1",
                        "tool_name": "tool_result_query",
                        "arguments": [
                            "source_call_id": "source-1",
                            "mode": "search",
                            "search": "DECISION",
                        ],
                        "side_effecting": false,
                        "requires_approval": false,
                    ]],
                ],
            ]
        )
        let largeActivity = try XCTUnwrap(
            NativeLocalAgentRunActivityProjector.project(run: largeResult)
        )
        XCTAssertEqual(largeActivity.kind, .readingLargeResult)
        XCTAssertEqual(largeActivity.operation, "tool_result_query")
        XCTAssertEqual(largeActivity.detail, "search · DECISION")

        let capability = try run(
            status: "waiting_tool_result",
            extra: [
                "pending_tool_batch": [
                    "batch_id": "batch-2",
                    "calls": [[
                        "call_id": "call-2",
                        "tool_name": "capability_describe",
                        "arguments": ["plugin_option": "plugin_1", "tool_option": "tool_3"],
                        "side_effecting": false,
                        "requires_approval": false,
                    ]],
                ],
            ]
        )
        let capabilityActivity = try XCTUnwrap(
            NativeLocalAgentRunActivityProjector.project(run: capability)
        )
        XCTAssertEqual(capabilityActivity.kind, .restoringCapability)
        XCTAssertEqual(capabilityActivity.detail, "plugin_1 / tool_3")

        let waitingUser = try run(status: "waiting_user")
        XCTAssertEqual(
            NativeLocalAgentRunActivityProjector.project(run: waitingUser)?.kind,
            .waitingUser
        )
    }

    func testCarriesSelectedServiceURLIntoFollowingModelPhase() throws {
        let value = try run(
            status: "model_running",
            extra: [
                "continuation_input": [
                    "type": "tool_results",
                    "invocations": [[
                        "tool_name": "task_run_process_allocate_service",
                        "arguments": ["service_name": "backend"],
                        "result": [
                            "allocated": true,
                            "port": 18_081,
                            "service_url": "http://127.0.0.1:18081",
                        ],
                    ]],
                ],
            ]
        )
        let activity = try XCTUnwrap(
            NativeLocalAgentRunActivityProjector.project(run: value)
        )
        XCTAssertEqual(activity.kind, .runningModel)
        XCTAssertEqual(activity.detail, "http://127.0.0.1:18081")
    }

    func testUntrustedLargeNumericArgumentsDoNotTrapActivityProjection() throws {
        let value = try run(
            status: "waiting_tool_result",
            extra: [
                "pending_tool_batch": [
                    "batch_id": "batch-large-number",
                    "calls": [[
                        "call_id": "call-large-number",
                        "tool_name": "task_run_process_allocate_service",
                        "arguments": [
                            "service_name": "backend",
                            "preferred_port": 1e300,
                        ],
                        "side_effecting": false,
                        "requires_approval": false,
                    ]],
                ],
            ]
        )

        let activity = try XCTUnwrap(
            NativeLocalAgentRunActivityProjector.project(run: value)
        )
        XCTAssertEqual(activity.kind, .startingProcess)
        XCTAssertEqual(activity.detail, "backend")
    }

    private func run(status: String, extra: [String: Any] = [:]) throws -> LocalAgentRunRecord {
        var object: [String: Any] = [
            "run_id": "run-1",
            "owner_user_id": "user-1",
            "owner_entity_type": "task",
            "owner_entity_id": "task-1",
            "profile_key": "task_execution",
            "capability_policy_revision": "policy-1",
            "input": [:],
            "status": status,
            "version": 1,
            "created_at_unix_ms": 1_000,
            "updated_at_unix_ms": 2_000,
        ]
        extra.forEach { object[$0.key] = $0.value }
        return try JSONDecoder().decode(
            LocalAgentRunRecord.self,
            from: JSONSerialization.data(withJSONObject: object)
        )
    }
}
