import Foundation

public struct ConversationAskUserPromptRefreshToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

public struct ConversationAskUserPromptActionToken: Sendable, Equatable {
    public let promptID: String
    public let requestID: UUID

    public init(promptID: String, requestID: UUID) {
        self.promptID = promptID
        self.requestID = requestID
    }
}

/// Pure request identity for Ask User refreshes and per-prompt mutations.
/// A mutation invalidates snapshots that may still contain the old pending
/// value, while operations for unrelated prompts remain independent.
public struct ConversationAskUserPromptLifecycleState: Sendable, Equatable {
    public private(set) var activeRefreshToken: ConversationAskUserPromptRefreshToken?
    private var lastRefreshGeneration: UInt64 = 0
    private var actionRequestIDsByPromptID: [String: UUID] = [:]

    public init() {}

    public var activePromptIDs: Set<String> {
        Set(actionRequestIDsByPromptID.keys)
    }

    public mutating func beginRefresh() -> ConversationAskUserPromptRefreshToken {
        lastRefreshGeneration &+= 1
        let token = ConversationAskUserPromptRefreshToken(
            generation: lastRefreshGeneration,
            requestID: UUID()
        )
        activeRefreshToken = token
        return token
    }

    public func accepts(_ token: ConversationAskUserPromptRefreshToken) -> Bool {
        activeRefreshToken == token
    }

    @discardableResult
    public mutating func finishRefresh(
        _ token: ConversationAskUserPromptRefreshToken
    ) -> Bool {
        guard accepts(token) else { return false }
        activeRefreshToken = nil
        return true
    }

    public mutating func beginAction(
        promptID: String
    ) -> ConversationAskUserPromptActionToken? {
        guard actionRequestIDsByPromptID[promptID] == nil else { return nil }
        invalidateRefresh()
        let requestID = UUID()
        actionRequestIDsByPromptID[promptID] = requestID
        return .init(promptID: promptID, requestID: requestID)
    }

    public func accepts(_ token: ConversationAskUserPromptActionToken) -> Bool {
        actionRequestIDsByPromptID[token.promptID] == token.requestID
    }

    @discardableResult
    public mutating func finishAction(
        _ token: ConversationAskUserPromptActionToken
    ) -> Bool {
        guard accepts(token) else { return false }
        actionRequestIDsByPromptID.removeValue(forKey: token.promptID)
        invalidateRefresh()
        return true
    }

    public mutating func invalidateRefresh() {
        activeRefreshToken = nil
    }

    public mutating func reset() {
        invalidateRefresh()
        actionRequestIDsByPromptID.removeAll()
    }
}
