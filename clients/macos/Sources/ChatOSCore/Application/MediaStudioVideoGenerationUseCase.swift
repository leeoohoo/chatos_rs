import Foundation

public protocol MediaStudioVideoHistoryPersisting: Sendable {
    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String
    ) async throws -> MediaStudioVideoHistoryItem
}

public enum MediaStudioVideoGenerationOutcome: Equatable, Sendable {
    case saved(MediaStudioVideoHistoryItem)
    case generatedWithoutPersistence(persistenceErrorMessage: String)
}

public struct MediaStudioVideoGenerationUseCase: Sendable {
    private let generator: any MediaVideoGenerating
    private let history: any MediaStudioVideoHistoryPersisting

    public init(
        generator: any MediaVideoGenerating,
        history: any MediaStudioVideoHistoryPersisting
    ) {
        self.generator = generator
        self.history = history
    }

    public func generate(
        request: VideoGenerationRequest,
        owner ownerInput: String,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> MediaStudioVideoGenerationOutcome {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw MediaStudioVideoGenerationError.invalidOwner }

        let result = try await generator.generateVideo(request, progress: progress)
        // A provider may finish after cancellation without throwing. Never persist that result.
        try Task.checkCancellation()
        await progress(.init(status: "saving"))
        try Task.checkCancellation()

        do {
            let item = try await history.saveVideo(
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
                persistenceErrorMessage: error.localizedDescription
            )
        }
    }
}

public enum MediaStudioVideoGenerationError: LocalizedError, Equatable, Sendable {
    case invalidOwner

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "视频生成缺少有效的账号上下文。"
        }
    }
}

public struct MediaStudioVideoGenerationToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let requestID: UUID

    public init(ownerUserID: String, accountSessionID: UUID, requestID: UUID) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.requestID = requestID
    }
}

public struct MediaStudioVideoGenerationState: Equatable, Sendable {
    public private(set) var activeToken: MediaStudioVideoGenerationToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID
    ) -> MediaStudioVideoGenerationToken {
        let token = MediaStudioVideoGenerationToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: MediaStudioVideoGenerationToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: MediaStudioVideoGenerationToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
