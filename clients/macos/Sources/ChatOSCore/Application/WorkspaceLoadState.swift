import Foundation

public struct WorkspaceLoadToken: Sendable, Equatable {
    public let ownerUserID: String
    public let accountGeneration: UInt64
    public let requestID: UUID

    public init(ownerUserID: String, accountGeneration: UInt64, requestID: UUID) {
        self.ownerUserID = ownerUserID
        self.accountGeneration = accountGeneration
        self.requestID = requestID
    }
}

public struct WorkspaceLoadState: Sendable, Equatable {
    public private(set) var activeToken: WorkspaceLoadToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountGeneration: UInt64
    ) -> WorkspaceLoadToken {
        let token = WorkspaceLoadToken(
            ownerUserID: ownerUserID,
            accountGeneration: accountGeneration,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: WorkspaceLoadToken,
        ownerUserID: String?,
        accountGeneration: UInt64
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountGeneration == accountGeneration
    }

    @discardableResult
    public mutating func finish(_ token: WorkspaceLoadToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
