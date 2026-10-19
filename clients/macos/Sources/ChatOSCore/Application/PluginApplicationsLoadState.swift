import Foundation

public struct PluginApplicationsLoadToken: Sendable, Equatable {
    public let ownerUserID: String
    public let accountGeneration: UInt64
    public let requestID: UUID

    public init(ownerUserID: String, accountGeneration: UInt64, requestID: UUID) {
        self.ownerUserID = ownerUserID
        self.accountGeneration = accountGeneration
        self.requestID = requestID
    }
}

public struct PluginApplicationsLoadState: Sendable, Equatable {
    public private(set) var activeToken: PluginApplicationsLoadToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountGeneration: UInt64
    ) -> PluginApplicationsLoadToken {
        let token = PluginApplicationsLoadToken(
            ownerUserID: ownerUserID,
            accountGeneration: accountGeneration,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: PluginApplicationsLoadToken,
        ownerUserID: String?,
        accountGeneration: UInt64
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountGeneration == accountGeneration
    }

    @discardableResult
    public mutating func finish(_ token: PluginApplicationsLoadToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}

public struct PluginApplicationListUseCase: Sendable {
    private let service: any LocalConnectorPluginApplicationListing

    public init(service: any LocalConnectorPluginApplicationListing) {
        self.service = service
    }

    public func load() async throws -> [LocalConnectorPluginApplication] {
        try await service.fetchPluginApplications()
    }
}
