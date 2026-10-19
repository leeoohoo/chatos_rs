import Foundation

public struct ConversationSendToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

/// Identity for the one Conversation send allowed by the composer. Concrete
/// Tasks and optimistic presentation stay in the platform adapter; late Host
/// responses cannot complete or recover a send after it has been invalidated.
public struct ConversationSendLifecycleState: Sendable, Equatable {
    public private(set) var activeToken: ConversationSendToken?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func begin() -> ConversationSendToken {
        lastGeneration &+= 1
        let token = ConversationSendToken(
            generation: lastGeneration,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(_ token: ConversationSendToken) -> Bool {
        activeToken == token
    }

    @discardableResult
    public mutating func finish(_ token: ConversationSendToken) -> Bool {
        guard accepts(token) else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
