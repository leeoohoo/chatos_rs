@testable import ChatOSConnector
import ChatOSCore
import XCTest

final class NativeCompanionRelayTests: XCTestCase {
    func testCompanionRelayConcurrencyIsBounded() {
        XCTAssertEqual(
            NativeLocalConnectorService.maximumConcurrentCompanionRelayRequests,
            32
        )
    }

    func testCompanionAgentDrainTriggersCoalesceToOneRerun() async {
        let coordinator = CompanionAgentDrainCoordinator()
        let probe = CompanionAgentDrainProbe()

        await coordinator.schedule(ownerUserID: "owner") {
            await probe.run()
        }
        await probe.waitForCalls(1)
        await coordinator.schedule(ownerUserID: "owner") {
            await probe.run()
        }
        await coordinator.schedule(ownerUserID: "owner") {
            await probe.run()
        }

        let activeTaskCount = await coordinator.activeTaskCount()
        XCTAssertEqual(activeTaskCount, 1)
        await probe.resumeFirstCall()
        await probe.waitForCalls(2)
        await waitUntilCompanionDrainIdle(coordinator)
        let callCount = await probe.callCount()
        XCTAssertEqual(callCount, 2)
    }

    func testCompanionAgentSummaryUsesSanitizedSnakeCaseContract() throws {
        let summary = LocalConnectorCompanionAgentSummary(
            id: "agent-1",
            name: "开发 Agent",
            description: "负责实现功能",
            professionKey: "software_engineer",
            status: "active",
            heartbeatEnabled: true,
            lastHeartbeatAtUnixMs: 123,
            updatedAtUnixMs: 456
        )
        let object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: JSONEncoder().encode(summary)) as? [String: Any]
        )

        XCTAssertEqual(object["profession_key"] as? String, "software_engineer")
        XCTAssertEqual(object["heartbeat_enabled"] as? Bool, true)
        XCTAssertEqual(object["updated_at_unix_ms"] as? Int, 456)
        XCTAssertNil(object["role_prompt"])
        XCTAssertNil(object["model_config_id"])
        XCTAssertNil(object["default_plugin_ids"])
    }

    func testGatewayRoutesEveryCompanionRelayRequestType() {
        for messageType in [
            "companion_resources_request",
            "companion_resolve_resource_request",
            "companion_conversation_request",
            "companion_conversation_history_request",
            "companion_conversation_state_request",
            "companion_conversation_send_request",
            "companion_conversation_guidance_request",
            "companion_conversation_stop_request",
            "companion_message_tasks_request",
            "companion_ask_user_prompts_request",
            "companion_ask_user_submit_request",
            "companion_ask_user_cancel_request",
            "companion_agent_workspace_request",
            "companion_agent_conversation_request",
            "companion_agent_messages_request",
            "companion_agent_send_message_request",
            "companion_agent_open_direct_request",
            "companion_approvals_request",
            "companion_resolve_approval_request",
        ] {
            XCTAssertTrue(
                NativeLocalConnectorService.isCompanionRelayMessageType(messageType),
                "message type must be routed to the Companion relay handler: \(messageType)"
            )
        }

        for messageType in ["connected", "unknown_request", "companion_unknown_request"] {
            XCTAssertFalse(NativeLocalConnectorService.isCompanionRelayMessageType(messageType))
        }
    }

    func testApprovalPresentationRedactsAbsoluteDirectoryAndUnsupportedDecisions() {
        let approval = LocalConnectorPendingApproval(
            id: "approval-1",
            requestID: "request-1",
            command: "git status",
            cwd: "/Users/alice/private/workspace/repository",
            source: "terminal",
            risk: "medium",
            reason: "需要读取工作区状态",
            createdAt: "2026-09-15T10:00:00Z",
            availableDecisions: ["accept", "acceptForSession", "decline", "approve", "unknown"]
        )

        let visible = NativeLocalConnectorService.companionApproval(approval)

        XCTAssertEqual(visible.context, "repository")
        XCTAssertFalse(visible.context?.contains("/Users/alice") == true)
        XCTAssertEqual(visible.availableDecisions, ["accept", "acceptForSession", "decline"])
        XCTAssertEqual(visible.command, "git status")
        XCTAssertEqual(visible.reason, "需要读取工作区状态")
    }

    func testApprovalDecisionAllowlistIsExactAndCaseSensitive() {
        for decision in ["accept", "acceptForSession", "decline"] {
            XCTAssertTrue(NativeLocalConnectorService.isCompanionApprovalDecision(decision))
        }
        for decision in ["", "approve", "accept_for_session", "Accept", "decline ", "unknown"] {
            XCTAssertFalse(NativeLocalConnectorService.isCompanionApprovalDecision(decision))
        }
    }

    func testResolvedOrUnknownApprovalReturnsNotFound() {
        XCTAssertThrowsError(try NativeLocalConnectorService.validateCompanionApprovalResolution(
            id: "already-resolved",
            decision: "accept",
            pending: []
        )) { error in
            XCTAssertEqual((error as? NativeCompanionRelayError)?.status, 404)
        }
    }

    func testDecisionMustAlsoBeOfferedByThePendingApproval() {
        let approval = LocalConnectorPendingApproval(
            id: "approval-1",
            requestID: "request-1",
            command: "git status",
            cwd: "/workspace/repository",
            source: "terminal",
            risk: "low",
            reason: nil,
            createdAt: "2026-09-15T10:00:00Z",
            availableDecisions: ["decline"]
        )

        XCTAssertThrowsError(try NativeLocalConnectorService.validateCompanionApprovalResolution(
            id: approval.id,
            decision: "accept",
            pending: [approval]
        )) { error in
            XCTAssertEqual((error as? NativeCompanionRelayError)?.status, 400)
        }
    }
}

private actor CompanionAgentDrainProbe {
    private var calls = 0
    private var firstCallContinuation: CheckedContinuation<Void, Never>?

    func run() async {
        calls += 1
        guard calls == 1 else { return }
        await withCheckedContinuation { continuation in
            firstCallContinuation = continuation
        }
    }

    func waitForCalls(_ expected: Int) async {
        while calls < expected {
            try? await Task.sleep(for: .milliseconds(5))
        }
    }

    func resumeFirstCall() {
        firstCallContinuation?.resume()
        firstCallContinuation = nil
    }

    func callCount() -> Int { calls }
}

private func waitUntilCompanionDrainIdle(
    _ coordinator: CompanionAgentDrainCoordinator
) async {
    for _ in 0..<100 {
        if await coordinator.activeTaskCount() == 0 { return }
        try? await Task.sleep(for: .milliseconds(5))
    }
}
