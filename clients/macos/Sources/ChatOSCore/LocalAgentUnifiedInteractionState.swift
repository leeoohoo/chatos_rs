// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

/// Presents Main Chat and Task Runner interactions through one native service
/// boundary while preserving their separate presentation projections.
public actor LocalAgentUnifiedInteractionState:
    LocalAgentAskUserStateStoring,
    LocalAgentRunControlStateStoring
{
    private let mainChat: any LocalAgentAskUserStateStoring & LocalAgentRunControlStateStoring
    private let taskRunner: any LocalAgentAskUserStateStoring & LocalAgentRunControlStateStoring

    public init(
        mainChat: any LocalAgentAskUserStateStoring & LocalAgentRunControlStateStoring,
        taskRunner: any LocalAgentAskUserStateStoring & LocalAgentRunControlStateStoring
    ) {
        self.mainChat = mainChat
        self.taskRunner = taskRunner
    }

    public func localAgentPrompts(
        sessionID: String,
        limit: Int
    ) async throws -> [AskUserPrompt] {
        async let main = mainChat.localAgentPrompts(sessionID: sessionID, limit: limit)
        async let tasks = taskRunner.localAgentPrompts(sessionID: sessionID, limit: limit)
        return try await merge(
            main,
            tasks,
            id: \.id,
            limit: limit,
            order: promptOrder
        )
    }

    public func localAgentPromptRoute(
        promptID: String,
        sessionID: String
    ) async throws -> LocalAgentAskUserRoute {
        do {
            return try await mainChat.localAgentPromptRoute(
                promptID: promptID,
                sessionID: sessionID
            )
        } catch LocalAgentConversationHistoryError.promptUnavailable {
            return try await taskRunner.localAgentPromptRoute(
                promptID: promptID,
                sessionID: sessionID
            )
        }
    }

    public func updateLocalAgentPromptStatus(
        promptID: String,
        sessionID: String,
        status: AskUserPromptStatus
    ) async throws -> AskUserPrompt {
        do {
            _ = try await mainChat.localAgentPromptRoute(
                promptID: promptID,
                sessionID: sessionID
            )
            return try await mainChat.updateLocalAgentPromptStatus(
                promptID: promptID,
                sessionID: sessionID,
                status: status
            )
        } catch LocalAgentConversationHistoryError.promptUnavailable {
            return try await taskRunner.updateLocalAgentPromptStatus(
                promptID: promptID,
                sessionID: sessionID,
                status: status
            )
        }
    }

    public func localAgentRunControls(
        sessionID: String
    ) async -> [LocalAgentRunControlState] {
        async let main = mainChat.localAgentRunControls(sessionID: sessionID)
        async let tasks = taskRunner.localAgentRunControls(sessionID: sessionID)
        return await merge(
            main,
            tasks,
            id: \.runID,
            order: controlOrder
        )
    }

    public func localAgentPendingToolApprovals(
        sessionID: String
    ) async -> [LocalAgentToolApprovalRequest] {
        async let main = mainChat.localAgentPendingToolApprovals(sessionID: sessionID)
        async let tasks = taskRunner.localAgentPendingToolApprovals(sessionID: sessionID)
        return await merge(
            main,
            tasks,
            id: \.invocationID,
            order: { $0.invocationID < $1.invocationID }
        )
    }

    public func requireLocalAgentRunControl(
        runID: String,
        sessionID: String
    ) async throws -> LocalAgentRunControlState {
        do {
            return try await mainChat.requireLocalAgentRunControl(
                runID: runID,
                sessionID: sessionID
            )
        } catch LocalAgentConversationHistoryError.runUnavailable {
            return try await taskRunner.requireLocalAgentRunControl(
                runID: runID,
                sessionID: sessionID
            )
        }
    }

    public func requireLocalAgentToolApproval(
        invocationID: String,
        sessionID: String
    ) async throws -> LocalAgentToolApprovalRequest {
        do {
            return try await mainChat.requireLocalAgentToolApproval(
                invocationID: invocationID,
                sessionID: sessionID
            )
        } catch LocalAgentConversationHistoryError.toolApprovalUnavailable {
            return try await taskRunner.requireLocalAgentToolApproval(
                invocationID: invocationID,
                sessionID: sessionID
            )
        }
    }

    private func merge<Value, ID: Hashable>(
        _ first: [Value],
        _ second: [Value],
        id: KeyPath<Value, ID>,
        limit: Int? = nil,
        order: (Value, Value) -> Bool
    ) -> [Value] {
        var values: [ID: Value] = [:]
        for value in first + second {
            let key = value[keyPath: id]
            if values[key] == nil { values[key] = value }
        }
        let sorted = values.values.sorted(by: order)
        guard let limit, limit >= 0, sorted.count > limit else { return sorted }
        return Array(sorted.suffix(limit))
    }

    private func promptOrder(_ lhs: AskUserPrompt, _ rhs: AskUserPrompt) -> Bool {
        let left = lhs.createdAt ?? lhs.updatedAt ?? .distantPast
        let right = rhs.createdAt ?? rhs.updatedAt ?? .distantPast
        if left != right { return left < right }
        return lhs.id < rhs.id
    }

    private func controlOrder(
        _ lhs: LocalAgentRunControlState,
        _ rhs: LocalAgentRunControlState
    ) -> Bool {
        let left = lhs.updatedAt ?? .distantPast
        let right = rhs.updatedAt ?? .distantPast
        if left != right { return left < right }
        return lhs.runID < rhs.runID
    }
}
