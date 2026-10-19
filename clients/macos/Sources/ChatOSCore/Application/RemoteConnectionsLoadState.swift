import Foundation

public struct RemoteConnectionsLoadToken: Sendable, Equatable {
    public let ownerUserID: String
    public let accountGeneration: UInt64
    public let requestID: UUID

    public init(ownerUserID: String, accountGeneration: UInt64, requestID: UUID) {
        self.ownerUserID = ownerUserID
        self.accountGeneration = accountGeneration
        self.requestID = requestID
    }
}

public struct RemoteConnectionsLoadState: Sendable, Equatable {
    public private(set) var activeToken: RemoteConnectionsLoadToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountGeneration: UInt64
    ) -> RemoteConnectionsLoadToken {
        let token = RemoteConnectionsLoadToken(
            ownerUserID: ownerUserID,
            accountGeneration: accountGeneration,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: RemoteConnectionsLoadToken,
        ownerUserID: String?,
        accountGeneration: UInt64
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountGeneration == accountGeneration
    }

    @discardableResult
    public mutating func finish(_ token: RemoteConnectionsLoadToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}

public struct RemoteConnectionListUseCase: Sendable {
    private let service: any RemoteConnectionListing

    public init(service: any RemoteConnectionListing) {
        self.service = service
    }

    public func load() async throws -> [RemoteConnection] {
        try await service.listConnections().sorted {
            $0.name.localizedStandardCompare($1.name) == .orderedAscending
        }
    }
}
