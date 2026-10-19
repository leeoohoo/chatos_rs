import Foundation

public struct ConversationSessionActivation: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

/// Pure identity for one visible activation of a cached Conversation session.
/// Concrete realtime and refresh Tasks stay in the UI adapter, while callbacks
/// from an earlier activation are rejected after deactivation or reactivation.
public struct ConversationSessionActivationState: Sendable, Equatable {
    public private(set) var activeSession: ConversationSessionActivation?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func activate() -> ConversationSessionActivation {
        if let activeSession {
            return activeSession
        }
        lastGeneration &+= 1
        let session = ConversationSessionActivation(
            generation: lastGeneration,
            requestID: UUID()
        )
        activeSession = session
        return session
    }

    public func accepts(_ session: ConversationSessionActivation) -> Bool {
        activeSession == session
    }

    public mutating func deactivate() {
        activeSession = nil
    }
}
