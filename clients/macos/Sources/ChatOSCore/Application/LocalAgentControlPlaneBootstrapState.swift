import Foundation

public struct LocalAgentControlPlaneBootstrapToken: Sendable, Equatable {
    public let ownerUserID: String
    public let accountGeneration: UInt64
    public let lifecycleGeneration: UInt64
    public let requestID: UUID

    public init(
        ownerUserID: String,
        accountGeneration: UInt64,
        lifecycleGeneration: UInt64,
        requestID: UUID
    ) {
        self.ownerUserID = ownerUserID
        self.accountGeneration = accountGeneration
        self.lifecycleGeneration = lifecycleGeneration
        self.requestID = requestID
    }
}

public struct LocalAgentControlPlaneBootstrapState: Sendable, Equatable {
    public private(set) var activeToken: LocalAgentControlPlaneBootstrapToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountGeneration: UInt64,
        lifecycleGeneration: UInt64
    ) -> LocalAgentControlPlaneBootstrapToken {
        let token = LocalAgentControlPlaneBootstrapToken(
            ownerUserID: ownerUserID,
            accountGeneration: accountGeneration,
            lifecycleGeneration: lifecycleGeneration,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: LocalAgentControlPlaneBootstrapToken,
        ownerUserID: String?,
        accountGeneration: UInt64,
        lifecycleGeneration: UInt64
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountGeneration == accountGeneration
            && token.lifecycleGeneration == lifecycleGeneration
    }

    @discardableResult
    public mutating func finish(_ token: LocalAgentControlPlaneBootstrapToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
