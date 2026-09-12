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

public struct LocalAgentMainChatRunRecovery: Equatable, Sendable {
    public var binding: LocalAgentMainChatRunBinding
    public var detail: LocalAgentRunDetail

    public init(binding: LocalAgentMainChatRunBinding, detail: LocalAgentRunDetail) {
        self.binding = binding
        self.detail = detail
    }
}

/// Replaces the in-memory Main Chat projection from authoritative Host data
/// before incremental UI-event delivery resumes after an app or Host restart.
public protocol LocalAgentMainChatStateRestoring: Sendable {
    func restoreLocalAgentMainChatRuns(
        _ recoveries: [LocalAgentMainChatRunRecovery]
    ) async throws
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
