import Foundation

public struct ConversationRuntimeRecoveryToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

/// Identity for restoring one Conversation after its local runtime becomes
/// ready. A newer ready signal or an unavailable transition invalidates every
/// older recovery, even when an underlying service ignores Task cancellation.
public struct ConversationRuntimeRecoveryState: Sendable, Equatable {
    public private(set) var activeToken: ConversationRuntimeRecoveryToken?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func begin() -> ConversationRuntimeRecoveryToken {
        lastGeneration &+= 1
        let token = ConversationRuntimeRecoveryToken(
            generation: lastGeneration,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(_ token: ConversationRuntimeRecoveryToken) -> Bool {
        activeToken == token
    }

    @discardableResult
    public mutating func finish(_ token: ConversationRuntimeRecoveryToken) -> Bool {
        guard accepts(token) else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
