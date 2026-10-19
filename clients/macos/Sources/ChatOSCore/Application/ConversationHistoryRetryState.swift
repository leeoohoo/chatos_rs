import Foundation

public struct ConversationHistoryRetryToken: Sendable, Equatable {
    public let activation: ConversationSessionActivation
    public let attempt: Int
    public let requestID: UUID

    public init(
        activation: ConversationSessionActivation,
        attempt: Int,
        requestID: UUID
    ) {
        self.activation = activation
        self.attempt = attempt
        self.requestID = requestID
    }
}

public struct ConversationHistoryRetrySchedule: Sendable, Equatable {
    public let token: ConversationHistoryRetryToken
    public let delay: Duration

    public init(token: ConversationHistoryRetryToken, delay: Duration) {
        self.token = token
        self.delay = delay
    }
}

/// Pure retry budget and request identity for one visible Conversation.
/// Concrete sleep Tasks remain in the UI adapter, while a deactivated or
/// superseded activation cannot wake and enqueue work for the current page.
public struct ConversationHistoryRetryState: Sendable, Equatable {
    public private(set) var activeToken: ConversationHistoryRetryToken?
    public private(set) var nextAttempt = 0

    public init() {}

    public mutating func schedule(
        activation: ConversationSessionActivation
    ) -> ConversationHistoryRetrySchedule? {
        guard activeToken == nil,
              let delay = ConversationHistoryUseCase.retryDelay(at: nextAttempt) else {
            return nil
        }
        let token = ConversationHistoryRetryToken(
            activation: activation,
            attempt: nextAttempt,
            requestID: UUID()
        )
        nextAttempt += 1
        activeToken = token
        return ConversationHistoryRetrySchedule(token: token, delay: delay)
    }

    public func accepts(
        _ token: ConversationHistoryRetryToken,
        activation: ConversationSessionActivation?
    ) -> Bool {
        activeToken == token && token.activation == activation
    }

    @discardableResult
    public mutating func finish(_ token: ConversationHistoryRetryToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func cancelScheduled() {
        activeToken = nil
    }

    public mutating func reset() {
        activeToken = nil
        nextAttempt = 0
    }
}
