// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public enum NativeLocalAgentAskUserPromptError: Error, Equatable, Sendable {
    case invalidLimit
    case invalidAnswer
}

extension NativeLocalAgentAskUserPromptError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .invalidLimit: "本地 Agent 提问数量必须为正数"
        case .invalidAnswer: "请先填写回复或选择一个选项"
        }
    }
}

/// Native Ask User service backed only by the account Local Agent Host and
/// the typed event state. No server Task Runner or Cloud Agent endpoint is
/// consulted.
public struct NativeLocalAgentAskUserPromptService: AskUserPromptServicing {
    private let accountSession: any NativeLocalAgentAccountSessionAccess
    private let state: any LocalAgentAskUserStateStoring

    public init(
        accountSession: any NativeLocalAgentAccountSessionAccess,
        state: any LocalAgentAskUserStateStoring
    ) {
        self.accountSession = accountSession
        self.state = state
    }

    public func fetchPrompts(
        sessionID: String,
        limit: Int = 100
    ) async throws -> [AskUserPrompt] {
        guard limit > 0 else { throw NativeLocalAgentAskUserPromptError.invalidLimit }
        return try await state.localAgentPrompts(sessionID: sessionID, limit: limit)
    }

    public func submit(
        promptID: String,
        sessionID: String,
        submission: AskUserSubmission
    ) async throws -> AskUserPrompt {
        let route = try await state.localAgentPromptRoute(
            promptID: promptID,
            sessionID: sessionID
        )
        let text = submission.values["answer"]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .nonEmpty
        let selectedOptionIDs: [String]
        switch submission.selection {
        case let .single(value):
            selectedOptionIDs = [value]
        case let .multiple(values):
            selectedOptionIDs = values
        case nil:
            selectedOptionIDs = []
        }
        guard text != nil || !selectedOptionIDs.isEmpty else {
            throw NativeLocalAgentAskUserPromptError.invalidAnswer
        }

        let client = try await accountSession.activeClient()
        _ = try await client.accepted(.answerUserQuestion(
            runID: route.runID,
            interactionID: route.interactionID,
            answer: LocalAgentUserAnswer(
                text: text,
                selectedOptionIDs: selectedOptionIDs
            )
        ))
        return try await state.updateLocalAgentPromptStatus(
            promptID: promptID,
            sessionID: sessionID,
            status: .ok
        )
    }

    public func cancel(
        promptID: String,
        sessionID: String
    ) async throws -> AskUserPrompt {
        let route = try await state.localAgentPromptRoute(
            promptID: promptID,
            sessionID: sessionID
        )
        let client = try await accountSession.activeClient()
        let run = try await client.run(id: route.runID)
        guard run.runID == route.runID,
              LocalAgentUIPresentation.pendingUserInteraction(run)?.interactionID
                == route.interactionID
        else {
            throw LocalAgentConversationHistoryError.promptUnavailable
        }
        _ = try await client.accepted(.cancelRun(
            runID: route.runID,
            expectedVersion: run.version
        ))
        return try await state.updateLocalAgentPromptStatus(
            promptID: promptID,
            sessionID: sessionID,
            status: .canceled
        )
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
