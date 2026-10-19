import Foundation

public struct MessageTaskRetryToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID
    public let contextID: String

    public init(generation: UInt64, requestID: UUID, contextID: String) {
        self.generation = generation
        self.requestID = requestID
        self.contextID = contextID
    }
}

/// Request identity for retrying one Task run. A retry response is allowed to
/// update presentation only while its target remains current and visible.
public struct MessageTaskRetryState: Sendable, Equatable {
    public private(set) var activeToken: MessageTaskRetryToken?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func begin(contextID: String) -> MessageTaskRetryToken {
        lastGeneration &+= 1
        let token = MessageTaskRetryToken(
            generation: lastGeneration,
            requestID: UUID(),
            contextID: contextID
        )
        activeToken = token
        return token
    }

    public func accepts(_ token: MessageTaskRetryToken) -> Bool {
        activeToken == token
    }

    @discardableResult
    public mutating func finish(_ token: MessageTaskRetryToken) -> Bool {
        guard accepts(token) else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
