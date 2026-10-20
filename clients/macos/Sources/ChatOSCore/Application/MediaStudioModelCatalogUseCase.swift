import Foundation

public struct MediaStudioModelCatalog: Equatable, Sendable {
    public var imageModels: [MediaGenerationModel]
    public var videoModels: [MediaGenerationModel]

    public init(
        imageModels: [MediaGenerationModel],
        videoModels: [MediaGenerationModel]
    ) {
        self.imageModels = imageModels
        self.videoModels = videoModels
    }
}

public struct MediaStudioModelCatalogUseCase: Sendable {
    private let provider: any MediaGenerationModelCatalogProviding

    public init(provider: any MediaGenerationModelCatalogProviding) {
        self.provider = provider
    }

    public func load() async throws -> MediaStudioModelCatalog {
        let models = try await provider.fetchModels()
        try Task.checkCancellation()
        return MediaStudioModelCatalog(
            imageModels: models,
            videoModels: models.filter(Self.isSupportedVideoModel)
        )
    }

    private static func isSupportedVideoModel(_ model: MediaGenerationModel) -> Bool {
        model.modelName.trimmingCharacters(in: .whitespacesAndNewlines)
            .caseInsensitiveCompare("MiniMax-H3") == .orderedSame
    }
}

public struct MediaStudioModelCatalogToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let requestID: UUID

    public init(ownerUserID: String, accountSessionID: UUID, requestID: UUID) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.requestID = requestID
    }
}

public struct MediaStudioModelCatalogState: Equatable, Sendable {
    public private(set) var activeToken: MediaStudioModelCatalogToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID
    ) -> MediaStudioModelCatalogToken {
        let token = MediaStudioModelCatalogToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: MediaStudioModelCatalogToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: MediaStudioModelCatalogToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
