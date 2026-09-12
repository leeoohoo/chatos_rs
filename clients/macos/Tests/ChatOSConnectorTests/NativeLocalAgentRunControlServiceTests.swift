// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native local Agent Run controls")
struct NativeLocalAgentRunControlServiceTests {
    @Test("pause resume and cancel target the exact local Run")
    func sendsRunControls() async throws {
        for (status, expectedType) in [
            (LocalAgentRunStatus.modelRunning, "pause_run"),
            (.paused, "resume_run"),
            (.waitingToolResult, "cancel_run"),
        ] {
            let fixture = try ControlFixture(status: status)
            switch expectedType {
            case "pause_run":
                try await fixture.service.pause(runID: "run-1", sessionID: "thread-1")
            case "resume_run":
                try await fixture.service.resume(runID: "run-1", sessionID: "thread-1")
            default:
                try await fixture.service.cancel(runID: "run-1", sessionID: "thread-1")
            }
            let command = try controlCommand(try #require(await fixture.transport.lastRequest()))
            #expect(command["type"] as? String == expectedType)
            let payload = try #require(command["payload"] as? [String: Any])
            #expect(payload["run_id"] as? String == "run-1")
        }
    }

    @Test("Ask User cannot be bypassed with a raw resume command")
    func refusesAskUserBypass() async throws {
        let fixture = try ControlFixture(status: .paused, interactionKind: "ask_user")

        await #expect(throws: NativeLocalAgentRunControlError.actionUnavailable) {
            try await fixture.service.resume(runID: "run-1", sessionID: "thread-1")
        }
        #expect(await fixture.transport.lastRequest() == nil)
    }

    @Test("tool decision uses the pending invocation from the same conversation")
    func decidesToolApproval() async throws {
        let fixture = try ControlFixture(status: .waitingToolResult)

        try await fixture.service.decideToolApproval(
            invocationID: "invocation-1",
            sessionID: "thread-1",
            decision: .reject,
            reason: "  user rejected  "
        )

        let command = try controlCommand(try #require(await fixture.transport.lastRequest()))
        #expect(command["type"] as? String == "decide_tool_approval")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["invocation_id"] as? String == "invocation-1")
        #expect(payload["decision"] as? String == "reject")
        #expect(payload["reason"] as? String == "user rejected")
    }

    @Test("a conversation cannot control another conversation's Run")
    func rejectsCrossConversationControl() async throws {
        let fixture = try ControlFixture(status: .modelRunning)

        await #expect(throws: LocalAgentConversationHistoryError.runUnavailable) {
            try await fixture.service.cancel(runID: "run-1", sessionID: "thread-other")
        }
        #expect(await fixture.transport.lastRequest() == nil)
    }
}

private struct ControlFixture {
    var service: NativeLocalAgentRunControlService
    var transport: ControlTransport

    init(status: LocalAgentRunStatus, interactionKind: String? = nil) throws {
        let state = ControlState(
            control: LocalAgentRunControlState(
                runID: "run-1",
                sessionID: "thread-1",
                turnID: "turn-1",
                status: status,
                iteration: 2,
                retryCount: 0,
                interactionKind: interactionKind
            ),
            approval: LocalAgentToolApprovalRequest(
                invocationID: "invocation-1",
                runID: "run-1",
                sessionID: "thread-1",
                turnID: "turn-1",
                toolName: "save_design",
                effect: .write,
                argumentsDigest: "sha256:args"
            )
        )
        let transport = ControlTransport()
        let client = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: transport
        )
        service = NativeLocalAgentRunControlService(
            accountSession: ControlAccountSession(client: client),
            state: state
        )
        self.transport = transport
    }
}

private actor ControlState: LocalAgentRunControlStateStoring {
    let control: LocalAgentRunControlState
    let approval: LocalAgentToolApprovalRequest

    init(
        control: LocalAgentRunControlState,
        approval: LocalAgentToolApprovalRequest
    ) {
        self.control = control
        self.approval = approval
    }

    func localAgentRunControls(sessionID: String) -> [LocalAgentRunControlState] {
        control.sessionID == sessionID ? [control] : []
    }

    func localAgentPendingToolApprovals(
        sessionID: String
    ) -> [LocalAgentToolApprovalRequest] {
        approval.sessionID == sessionID ? [approval] : []
    }

    func requireLocalAgentRunControl(
        runID: String,
        sessionID: String
    ) throws -> LocalAgentRunControlState {
        guard control.runID == runID, control.sessionID == sessionID else {
            throw LocalAgentConversationHistoryError.runUnavailable
        }
        return control
    }

    func requireLocalAgentToolApproval(
        invocationID: String,
        sessionID: String
    ) throws -> LocalAgentToolApprovalRequest {
        guard approval.invocationID == invocationID, approval.sessionID == sessionID else {
            throw LocalAgentConversationHistoryError.toolApprovalUnavailable
        }
        return approval
    }
}

private struct ControlAccountSession: NativeLocalAgentAccountSessionAccess {
    var client: NativeLocalAgentIPCClient

    func client(accountID: String) async throws -> NativeLocalAgentIPCClient { client }
    func activeClient() async throws -> NativeLocalAgentIPCClient { client }
    func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID: String
    ) async throws -> [LocalAgentAttachmentReference] { [] }
    func discardStagedAttachments(
        _ references: [LocalAgentAttachmentReference],
        accountID: String
    ) async {}
}

private actor ControlTransport: LocalAgentFrameTransport {
    private var request: Data?

    func exchange(_ request: Data) async throws -> Data {
        self.request = request
        let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
        let requestID = try #require(object["request_id"] as? String)
        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": requestID,
            "response": [
                "type": "accepted",
                "payload": ["operation_id": "operation-1"],
            ],
        ])
    }

    func lastRequest() -> Data? { request }
}

private func controlCommand(_ request: Data) throws -> [String: Any] {
    let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
    return try #require(object["command"] as? [String: Any])
}
