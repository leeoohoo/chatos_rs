// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

/// Account-level native UI boundary for durable Local Agent events.
///
/// Implementations must be idempotent by `eventSeq`: the Host deliberately
/// replays a whole unacknowledged page when applying any event or persisting
/// its cursor fails.
public protocol LocalAgentUIEventApplying: Sendable {
    func applyLocalAgentUIEvent(
        _ event: LocalAgentUIEvent,
        mainChatBinding: LocalAgentMainChatRunBinding?
    ) async throws
}

public protocol LocalAgentConversationUpdateStreaming: Sendable {
    func localAgentUpdates(sessionID: String) async -> AsyncStream<Void>
}

public struct LocalAgentAskUserRoute: Equatable, Sendable {
    public var runID: String
    public var interactionID: String

    public init(runID: String, interactionID: String) {
        self.runID = runID
        self.interactionID = interactionID
    }
}

public protocol LocalAgentAskUserStateStoring: Sendable {
    func localAgentPrompts(sessionID: String, limit: Int) async throws -> [AskUserPrompt]
    func localAgentPromptRoute(
        promptID: String,
        sessionID: String
    ) async throws -> LocalAgentAskUserRoute
    func updateLocalAgentPromptStatus(
        promptID: String,
        sessionID: String,
        status: AskUserPromptStatus
    ) async throws -> AskUserPrompt
}
