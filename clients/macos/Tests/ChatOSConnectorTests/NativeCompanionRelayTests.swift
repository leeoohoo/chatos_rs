@testable import ChatOSConnector
import ChatOSCore
import XCTest

final class NativeCompanionRelayTests: XCTestCase {
    func testGatewayRoutesEveryCompanionRelayRequestType() {
        for messageType in [
            "companion_resources_request",
            "companion_resolve_resource_request",
            "companion_approvals_request",
            "companion_resolve_approval_request",
        ] {
            XCTAssertTrue(
                NativeLocalConnectorService.isCompanionRelayMessageType(messageType),
                "message type must be routed to the Companion relay handler: \(messageType)"
            )
        }

        for messageType in ["connected", "terminal_exec_request", "companion_unknown_request"] {
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
