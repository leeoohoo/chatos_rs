import Foundation

public struct TurnProcessRequestToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID
    public let sessionID: String
    public let turnID: String

    public init(
        generation: UInt64,
        requestID: UUID,
        sessionID: String,
        turnID: String
    ) {
        self.generation = generation
        self.requestID = requestID
        self.sessionID = sessionID
        self.turnID = turnID
    }
}

/// Owns the identity of the request allowed to update one visible Turn Process
/// presentation. The concrete Task remains in the platform ViewModel.
public struct TurnProcessLifecycleState: Sendable, Equatable {
    private var lastGeneration: UInt64 = 0
    public private(set) var activeToken: TurnProcessRequestToken?

    public init() {}

    public mutating func begin(
        sessionID: String,
        turnID: String
    ) -> TurnProcessRequestToken {
        lastGeneration &+= 1
        let token = TurnProcessRequestToken(
            generation: lastGeneration,
            requestID: UUID(),
            sessionID: sessionID,
            turnID: turnID
        )
        activeToken = token
        return token
    }

    public func accepts(_ token: TurnProcessRequestToken) -> Bool {
        activeToken == token
    }

    @discardableResult
    public mutating func finish(_ token: TurnProcessRequestToken) -> Bool {
        guard accepts(token) else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
