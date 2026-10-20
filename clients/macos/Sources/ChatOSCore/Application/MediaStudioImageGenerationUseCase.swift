import Foundation

public struct MediaStudioImageHistoryItem: Identifiable, Equatable, Sendable {
    public var id: String
    public var prompt: String
    public var modelName: String
    public var createdAt: Date
    public var images: [GeneratedMediaAsset]

    public init(
        id: String,
        prompt: String,
        modelName: String,
        createdAt: Date,
        images: [GeneratedMediaAsset]
    ) {
        self.id = id
        self.prompt = prompt
        self.modelName = modelName
        self.createdAt = createdAt
        self.images = images
    }
}

public protocol MediaStudioImageHistoryPersisting: Sendable {
    func saveImage(
        _ result: ImageGenerationResult,
        prompt: String,
        owner: String
    ) async throws -> MediaStudioImageHistoryItem
}

public enum MediaStudioImageGenerationOutcome: Equatable, Sendable {
    case saved(MediaStudioImageHistoryItem)
    case generatedWithoutPersistence(
        result: ImageGenerationResult,
        persistenceErrorMessage: String
    )
}

public struct MediaStudioImageGenerationUseCase: Sendable {
    private let generator: any MediaImageGenerating
    private let history: any MediaStudioImageHistoryPersisting

    public init(
        generator: any MediaImageGenerating,
        history: any MediaStudioImageHistoryPersisting
    ) {
        self.generator = generator
        self.history = history
    }

    public func generate(
        request: ImageGenerationRequest,
        owner ownerInput: String
    ) async throws -> MediaStudioImageGenerationOutcome {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw MediaStudioImageGenerationError.invalidOwner }
        let result = try await generator.generateImage(request)
        try Task.checkCancellation()
        do {
            let item = try await history.saveImage(
                result,
                prompt: request.prompt,
                owner: owner
            )
            try Task.checkCancellation()
            return .saved(item)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            return .generatedWithoutPersistence(
                result: result,
                persistenceErrorMessage: error.localizedDescription
            )
        }
    }
}

public enum MediaStudioImageGenerationError: LocalizedError, Equatable, Sendable {
    case invalidOwner

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "图片生成缺少有效的账号上下文。"
        }
    }
}

public struct MediaStudioImageGenerationToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let requestID: UUID

    public init(ownerUserID: String, accountSessionID: UUID, requestID: UUID) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.requestID = requestID
    }
}

public struct MediaStudioImageGenerationState: Equatable, Sendable {
    public private(set) var activeToken: MediaStudioImageGenerationToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID
    ) -> MediaStudioImageGenerationToken {
        let token = MediaStudioImageGenerationToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: MediaStudioImageGenerationToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: MediaStudioImageGenerationToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
