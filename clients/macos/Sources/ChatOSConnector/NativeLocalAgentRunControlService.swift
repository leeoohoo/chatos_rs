// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public enum NativeLocalAgentRunControlError: Error, Equatable, Sendable {
    case actionUnavailable
}

extension NativeLocalAgentRunControlError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .actionUnavailable:
            "当前 Run 状态不允许执行这个操作"
        }
    }
}

/// Main Chat and Task Runner controls backed only by the account's Rust Local Agent Host.
/// Accepted commands are reflected back into UI exclusively through durable
/// typed events; this service never fabricates a successful state transition.
public struct NativeLocalAgentRunControlService: LocalAgentRunControlServicing {
    private let accountSession: any NativeLocalAgentAccountSessionAccess
    private let state: any LocalAgentRunControlStateStoring

    public init(
        accountSession: any NativeLocalAgentAccountSessionAccess,
        state: any LocalAgentRunControlStateStoring
    ) {
        self.accountSession = accountSession
        self.state = state
    }

    public func fetchRunControls(sessionID: String) async -> [LocalAgentRunControlState] {
        await state.localAgentRunControls(sessionID: sessionID)
    }

    public func fetchPendingToolApprovals(
        sessionID: String
    ) async -> [LocalAgentToolApprovalRequest] {
        await state.localAgentPendingToolApprovals(sessionID: sessionID)
    }

    public func pause(runID: String, sessionID: String) async throws {
        let control = try await state.requireLocalAgentRunControl(
            runID: runID,
            sessionID: sessionID
        )
        guard control.canPause else { throw NativeLocalAgentRunControlError.actionUnavailable }
        try await send(.pauseRun(runID: runID))
    }

    public func resume(runID: String, sessionID: String) async throws {
        let control = try await state.requireLocalAgentRunControl(
            runID: runID,
            sessionID: sessionID
        )
        guard control.canResume else { throw NativeLocalAgentRunControlError.actionUnavailable }
        try await send(.resumeRun(runID: runID))
    }

    public func cancel(runID: String, sessionID: String) async throws {
        let control = try await state.requireLocalAgentRunControl(
            runID: runID,
            sessionID: sessionID
        )
        guard control.canCancel else { throw NativeLocalAgentRunControlError.actionUnavailable }
        try await send(.cancelRun(runID: runID))
    }

    public func decideToolApproval(
        invocationID: String,
        sessionID: String,
        decision: LocalAgentToolApprovalDecision,
        reason: String?
    ) async throws {
        _ = try await state.requireLocalAgentToolApproval(
            invocationID: invocationID,
            sessionID: sessionID
        )
        let normalizedReason = reason?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .nilIfEmpty
        try await send(.decideToolApproval(
            invocationID: invocationID,
            decision: decision,
            reason: normalizedReason
        ))
    }

    private func send(_ command: LocalAgentCommand) async throws {
        let client = try await accountSession.activeClient()
        _ = try await client.accepted(command)
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
