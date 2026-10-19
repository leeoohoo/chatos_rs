import Foundation

public struct LocalConnectorApprovalLoadToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

/// Pure request identity for the approvals page's explicit snapshot load.
/// Streaming and consistency monitoring have a separate lifecycle and must not
/// be cancelled when this page-only request is replaced or leaves the screen.
public struct LocalConnectorApprovalLoadState: Sendable, Equatable {
    public private(set) var activeToken: LocalConnectorApprovalLoadToken?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func begin() -> LocalConnectorApprovalLoadToken {
        lastGeneration &+= 1
        let token = LocalConnectorApprovalLoadToken(
            generation: lastGeneration,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(_ token: LocalConnectorApprovalLoadToken) -> Bool {
        activeToken == token
    }

    @discardableResult
    public mutating func finish(_ token: LocalConnectorApprovalLoadToken) -> Bool {
        guard accepts(token) else { return false }
        activeToken = nil
        return true
    }

    public mutating func cancel() {
        activeToken = nil
    }
}
