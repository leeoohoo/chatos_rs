// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

public struct LocalAgentRunControlState: Identifiable, Equatable, Sendable {
    public var id: String { runID }
    public var runID: String
    public var runVersion: UInt64
    public var sessionID: String
    public var turnID: String
    public var status: LocalAgentRunStatus
    public var iteration: UInt32
    public var retryCount: UInt32
    public var interactionKind: String?
    public var reviewReason: String?
    public var updatedAt: Date?

    public init(
        runID: String,
        runVersion: UInt64,
        sessionID: String,
        turnID: String,
        status: LocalAgentRunStatus,
        iteration: UInt32,
        retryCount: UInt32,
        interactionKind: String? = nil,
        reviewReason: String? = nil,
        updatedAt: Date? = nil
    ) {
        self.runID = runID
        self.runVersion = runVersion
        self.sessionID = sessionID
        self.turnID = turnID
        self.status = status
        self.iteration = iteration
        self.retryCount = retryCount
        self.interactionKind = interactionKind
        self.reviewReason = reviewReason
        self.updatedAt = updatedAt
    }

    public var isTerminal: Bool {
        status == .succeeded || status == .failed || status == .cancelled
    }

    public var canPause: Bool {
        !isTerminal && status != .paused && status != .needsReview
    }

    public var canResume: Bool {
        status == .needsReview || (status == .paused && interactionKind != "ask_user")
    }

    public var requiresUserAnswer: Bool {
        status == .paused && interactionKind == "ask_user"
    }

    public var canCancel: Bool { !isTerminal }
}

public struct LocalAgentToolApprovalRequest: Identifiable, Equatable, Sendable {
    public var id: String { invocationID }
    public var invocationID: String
    public var runID: String
    public var sessionID: String
    public var turnID: String
    public var toolName: String
    public var effect: LocalAgentToolEffect
    public var argumentsDigest: String

    public init(
        invocationID: String,
        runID: String,
        sessionID: String,
        turnID: String,
        toolName: String,
        effect: LocalAgentToolEffect,
        argumentsDigest: String
    ) {
        self.invocationID = invocationID
        self.runID = runID
        self.sessionID = sessionID
        self.turnID = turnID
        self.toolName = toolName
        self.effect = effect
        self.argumentsDigest = argumentsDigest
    }
}

public protocol LocalAgentRunControlStateStoring: Sendable {
    func localAgentRunControls(sessionID: String) async -> [LocalAgentRunControlState]
    func localAgentPendingToolApprovals(
        sessionID: String
    ) async -> [LocalAgentToolApprovalRequest]
    func requireLocalAgentRunControl(
        runID: String,
        sessionID: String
    ) async throws -> LocalAgentRunControlState
    func requireLocalAgentToolApproval(
        invocationID: String,
        sessionID: String
    ) async throws -> LocalAgentToolApprovalRequest
}

public protocol LocalAgentRunControlServicing: Sendable {
    func fetchRunControls(sessionID: String) async -> [LocalAgentRunControlState]
    func fetchPendingToolApprovals(
        sessionID: String
    ) async -> [LocalAgentToolApprovalRequest]
    func pause(runID: String, sessionID: String) async throws
    func resume(runID: String, sessionID: String) async throws
    func cancel(runID: String, sessionID: String) async throws
    func decideToolApproval(
        invocationID: String,
        sessionID: String,
        decision: LocalAgentToolApprovalDecision,
        reason: String?
    ) async throws
}
