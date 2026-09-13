// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import XCTest
@testable import ChatOSConnector

final class NativeApprovalAgentTests: XCTestCase {
    func testApprovalUsesTypedLocalHostCommandAndObservesTerminalRun() async {
        let probe = ApprovalReviewProbe()
        let agent = NativeApprovalAgent(
            createReview: { owner, command in
                await probe.record(owner: owner, command: command)
                return Self.run(status: .queued)
            },
            loadRun: { _, _ in
                Self.run(
                    status: .succeeded,
                    terminalOutcome: .object([
                        "kind": .string("approval_decision"),
                        "decision": .string("approve"),
                        "reason": .string("范围受控"),
                        "remember_allow": .bool(true),
                    ])
                )
            },
            pollingInterval: .zero,
            maximumWait: .seconds(1)
        )
        let decision = await agent.evaluate(
            request: request(),
            ownerUserID: "user-1",
            modelConfigID: "model-1",
            thinkingLevel: "low"
        )
        XCTAssertEqual(decision, .approve(reason: "范围受控", rememberAllow: true))
        let captured = await probe.captured()
        XCTAssertEqual(captured?.owner, "user-1")
        XCTAssertEqual(captured?.command.reviewID, "approval-1")
        XCTAssertEqual(captured?.command.modelConfigID, "model-1")
        XCTAssertEqual(captured?.command.operation, "git status --short")
        XCTAssertEqual(captured?.command.reasoningEffort, "low")
    }

    func testInvalidOrFailedHostResultNeverApproves() async {
        for run in [
            Self.run(status: .failed, terminalOutcome: .object(["reason": .string("失败")])),
            Self.run(status: .succeeded, terminalOutcome: .object(["kind": .string("other")])),
        ] {
            let agent = NativeApprovalAgent(
                createReview: { _, _ in run },
                loadRun: { _, _ in XCTFail("terminal Run must not be polled"); return run },
                pollingInterval: .zero,
                maximumWait: .seconds(1)
            )
            guard case .askUser = await agent.evaluate(
                request: request(),
                ownerUserID: "user-1",
                modelConfigID: "model-1",
                thinkingLevel: nil
            ) else {
                return XCTFail("invalid and failed approval runs must ask the user")
            }
        }
    }

    private func request() -> NativeApprovalAgentRequest {
        .init(
            reviewID: "approval-1",
            command: "git",
            arguments: ["status", "--short"],
            cwd: "workspace",
            source: "shell",
            riskLevel: "low",
            riskReason: nil,
            requestedPermissionsDescription: "读取工作区状态"
        )
    }

    private static func run(
        status: LocalAgentRunStatus,
        terminalOutcome: LocalAgentJSONValue? = nil
    ) -> LocalAgentRunSnapshot {
        .init(
            runID: "run-1",
            profileKey: "approval_review",
            ownerUserID: "user-1",
            ownerEntityType: "approval",
            ownerEntityID: "approval-1",
            projectID: nil,
            status: status,
            version: 1,
            stepSeq: 0,
            iteration: 0,
            retryCount: 0,
            modelConfigID: "model-1",
            modelConfigRevision: 1,
            modelRuntimeSnapshot: .object([:]),
            contextStrategy: "provider_native",
            promptRevision: "approval-review-v1",
            capabilitySnapshotRef: "approval-decision-v1",
            terminalOutcome: terminalOutcome,
            createdAt: "2026-09-14T00:00:00Z",
            updatedAt: "2026-09-14T00:00:00Z"
        )
    }
}

private actor ApprovalReviewProbe {
    private var value: (owner: String, command: LocalAgentCreateApprovalReview)?

    func record(owner: String, command: LocalAgentCreateApprovalReview) {
        value = (owner, command)
    }

    func captured() -> (owner: String, command: LocalAgentCreateApprovalReview)? {
        value
    }
}
