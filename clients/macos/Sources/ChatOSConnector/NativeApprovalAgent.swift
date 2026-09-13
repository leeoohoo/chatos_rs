// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

enum NativeApprovalDecision: Sendable, Equatable {
    case approve(reason: String, rememberAllow: Bool)
    case deny(reason: String)
    case askUser(reason: String)
}

struct NativeApprovalAgentRequest: Sendable {
    var reviewID: String
    var command: String
    var arguments: [String]
    var cwd: String
    var source: String
    var riskLevel: String
    var riskReason: String?
    var requestedPermissionsDescription: String?
}

/// Thin UI observer for the shared Rust approval profile. It never receives
/// model credentials, calls a provider, executes tools, or owns an Agent loop.
struct NativeApprovalAgent: Sendable {
    typealias CreateReview = @Sendable (
        _ ownerUserID: String,
        _ command: LocalAgentCreateApprovalReview
    ) async throws -> LocalAgentRunSnapshot
    typealias LoadRun = @Sendable (
        _ ownerUserID: String,
        _ runID: String
    ) async throws -> LocalAgentRunSnapshot

    private let createReview: CreateReview
    private let loadRun: LoadRun
    private let pollingInterval: Duration
    private let maximumWait: Duration

    init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        self.init(
            createReview: { ownerUserID, command in
                let client = try await accountSession.client(accountID: ownerUserID)
                return try await client.createApprovalReview(command).run
            },
            loadRun: { ownerUserID, runID in
                let client = try await accountSession.client(accountID: ownerUserID)
                return try await client.run(id: runID)
            }
        )
    }

    init(
        createReview: @escaping CreateReview,
        loadRun: @escaping LoadRun,
        pollingInterval: Duration = .milliseconds(500),
        maximumWait: Duration = .seconds(900)
    ) {
        self.createReview = createReview
        self.loadRun = loadRun
        self.pollingInterval = pollingInterval
        self.maximumWait = maximumWait
    }

    func evaluate(
        request: NativeApprovalAgentRequest,
        ownerUserID: String,
        modelConfigID: String,
        thinkingLevel: String?
    ) async -> NativeApprovalDecision {
        do {
            let created = try await createReview(
                ownerUserID,
                LocalAgentCreateApprovalReview(
                    reviewID: request.reviewID,
                    modelConfigID: modelConfigID,
                    source: request.source,
                    cwd: request.cwd,
                    operation: ([request.command] + request.arguments).joined(separator: " "),
                    requestedPermissionsDescription: request.requestedPermissionsDescription,
                    riskLevel: request.riskLevel,
                    riskReason: request.riskReason,
                    reasoningEffort: thinkingLevel
                )
            )
            return try await waitForDecision(
                ownerUserID: ownerUserID,
                initial: created
            )
        } catch is CancellationError {
            return .askUser(reason: "本机审批已取消，已转交人工确认。")
        } catch {
            return .askUser(reason: "本机审批 Agent 不可用：\(error.localizedDescription)")
        }
    }

    private func waitForDecision(
        ownerUserID: String,
        initial: LocalAgentRunSnapshot
    ) async throws -> NativeApprovalDecision {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: maximumWait)
        var run = initial
        while true {
            if run.status == .succeeded {
                return Self.decision(from: run.terminalOutcome)
            }
            if run.status == .failed || run.status == .cancelled {
                let reason = run.terminalOutcome?.stringValue(forKey: "reason")
                    ?? "本机审批 Agent 未形成有效结论，已转交人工确认。"
                return .askUser(reason: reason)
            }
            if run.status == .needsReview || run.status == .paused {
                return .askUser(reason: "本机审批需要人工确认。")
            }
            guard clock.now < deadline else {
                return .askUser(reason: "本机审批等待超过 15 分钟，已转交人工确认。")
            }
            try Task.checkCancellation()
            try await clock.sleep(for: pollingInterval)
            run = try await loadRun(ownerUserID, run.runID)
        }
    }

    static func decision(from outcome: LocalAgentJSONValue?) -> NativeApprovalDecision {
        guard outcome?.stringValue(forKey: "kind") == "approval_decision",
              let rawDecision = outcome?.stringValue(forKey: "decision"),
              let reason = outcome?.stringValue(forKey: "reason")?.trimmedNonEmpty
        else {
            return .askUser(reason: "本机审批 Agent 返回了无效结论，已转交人工确认。")
        }
        switch rawDecision {
        case "approve":
            let remember = outcome?.objectValue?["remember_allow"]?.boolValue ?? false
            return .approve(reason: reason, rememberAllow: remember)
        case "deny":
            return .deny(reason: reason)
        case "ask_user":
            return .askUser(reason: reason)
        default:
            return .askUser(reason: "本机审批 Agent 返回了未知结论，已转交人工确认。")
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
