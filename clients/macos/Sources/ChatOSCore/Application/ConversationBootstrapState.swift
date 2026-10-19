import Foundation

public struct ConversationBootstrapToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

/// Identity for the one local Conversation hydration started at ViewModel
/// creation. Runtime transitions invalidate its final Host-facing recovery;
/// local History Store hydration may still finish independently.
public struct ConversationBootstrapState: Sendable, Equatable {
    public private(set) var activeToken: ConversationBootstrapToken?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func begin() -> ConversationBootstrapToken {
        lastGeneration &+= 1
        let token = ConversationBootstrapToken(
            generation: lastGeneration,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(_ token: ConversationBootstrapToken) -> Bool {
        activeToken == token
    }

    @discardableResult
    public mutating func finish(_ token: ConversationBootstrapToken) -> Bool {
        guard accepts(token) else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
