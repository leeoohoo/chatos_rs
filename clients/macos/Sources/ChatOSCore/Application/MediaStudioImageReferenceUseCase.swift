import Foundation

public protocol MediaStudioImageReferenceImporting: Sendable {
    func importLocalFiles(_ urls: [URL]) async throws -> [ImageGenerationInputImage]
    func importGeneratedAssets(
        _ assets: [GeneratedMediaAsset]
    ) async throws -> [ImageGenerationInputImage]
}

public struct MediaStudioImageReferenceUseCase: Sendable {
    public static let maximumReferenceCount = 8

    private let importer: any MediaStudioImageReferenceImporting

    public init(importer: any MediaStudioImageReferenceImporting) {
        self.importer = importer
    }

    public func importLocalFiles(
        _ urls: [URL],
        current: [ImageGenerationInputImage]
    ) async throws -> [ImageGenerationInputImage] {
        guard !urls.isEmpty else { return current }
        try validateCapacity(currentCount: current.count, requestedCount: urls.count)
        let imported = try await importer.importLocalFiles(urls)
        try Task.checkCancellation()
        return try merge(current: current, imported: imported)
    }

    public func importGeneratedAssets(
        _ assets: [GeneratedMediaAsset],
        authorizedBy history: [MediaStudioImageHistoryItem],
        current: [ImageGenerationInputImage]
    ) async throws -> [ImageGenerationInputImage] {
        guard !assets.isEmpty else { return current }
        let authorizedAssets = history.flatMap(\.images)
        guard assets.allSatisfy(authorizedAssets.contains) else {
            throw MediaStudioImageReferenceError.unauthorizedGeneratedAsset
        }
        try validateCapacity(currentCount: current.count, requestedCount: assets.count)
        let imported = try await importer.importGeneratedAssets(assets)
        try Task.checkCancellation()
        return try merge(current: current, imported: imported)
    }

    public func validateCapacity(currentCount: Int, requestedCount: Int) throws {
        guard currentCount >= 0,
              currentCount <= Self.maximumReferenceCount,
              requestedCount >= 0,
              requestedCount <= Self.maximumReferenceCount - currentCount else {
            throw MediaStudioImageReferenceError.tooManyReferences
        }
    }

    private func merge(
        current: [ImageGenerationInputImage],
        imported: [ImageGenerationInputImage]
    ) throws -> [ImageGenerationInputImage] {
        var merged = current
        for image in imported where !merged.contains(image) {
            merged.append(image)
        }
        guard merged.count <= Self.maximumReferenceCount else {
            throw MediaStudioImageReferenceError.tooManyReferences
        }
        return merged
    }
}

public enum MediaStudioImageReferenceError: LocalizedError, Equatable, Sendable {
    case tooManyReferences
    case unauthorizedGeneratedAsset

    public var errorDescription: String? {
        switch self {
        case .tooManyReferences:
            "一次最多选择 8 张参考图。"
        case .unauthorizedGeneratedAsset:
            "这张图片不属于当前账号的创作记录。"
        }
    }
}

public enum MediaStudioImageReferenceSource: Equatable, Sendable {
    case localFiles
    case generatedHistory
}

public struct MediaStudioImageReferenceToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let requestID: UUID
    public let source: MediaStudioImageReferenceSource

    public init(
        ownerUserID: String,
        accountSessionID: UUID,
        requestID: UUID,
        source: MediaStudioImageReferenceSource
    ) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.requestID = requestID
        self.source = source
    }
}

public struct MediaStudioImageReferenceState: Equatable, Sendable {
    public private(set) var activeToken: MediaStudioImageReferenceToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        source: MediaStudioImageReferenceSource
    ) -> MediaStudioImageReferenceToken {
        let token = MediaStudioImageReferenceToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            requestID: UUID(),
            source: source
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: MediaStudioImageReferenceToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: MediaStudioImageReferenceToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
