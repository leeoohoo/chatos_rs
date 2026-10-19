import Foundation

public struct ConversationTaskGraphAvailabilityToken: Sendable, Equatable {
    public let turnID: String
    public let revision: Int64
    public let generation: UInt64
    public let requestID: UUID

    public init(
        turnID: String,
        revision: Int64,
        generation: UInt64,
        requestID: UUID
    ) {
        self.turnID = turnID
        self.revision = revision
        self.generation = generation
        self.requestID = requestID
    }
}

/// Per-Turn identity for the lightweight Task Graph availability lookup.
/// Concrete Tasks and the successful revision cache remain in the platform
/// adapter; late responses cannot complete a replacement or cancelled lookup.
public struct ConversationTaskGraphAvailabilityState: Sendable, Equatable {
    public private(set) var activeTokens: [String: ConversationTaskGraphAvailabilityToken] = [:]
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func begin(
        turnID: String,
        revision: Int64
    ) -> ConversationTaskGraphAvailabilityToken {
        lastGeneration &+= 1
        let token = ConversationTaskGraphAvailabilityToken(
            turnID: turnID,
            revision: revision,
            generation: lastGeneration,
            requestID: UUID()
        )
        activeTokens[turnID] = token
        return token
    }

    public func accepts(_ token: ConversationTaskGraphAvailabilityToken) -> Bool {
        activeTokens[token.turnID] == token
    }

    @discardableResult
    public mutating func finish(
        _ token: ConversationTaskGraphAvailabilityToken
    ) -> Bool {
        guard accepts(token) else { return false }
        activeTokens[token.turnID] = nil
        return true
    }

    public mutating func invalidate(turnID: String) {
        activeTokens[turnID] = nil
    }

    public mutating func reset() {
        activeTokens.removeAll()
    }
}
