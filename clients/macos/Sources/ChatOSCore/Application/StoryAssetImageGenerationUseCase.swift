import Foundation

public struct StoryAssetImageGenerationCommand: Equatable, Sendable {
    public let projectID: UUID
    public let resourceID: String
    public let attemptID: UUID
    public let userIdeas: String

    public init(
        projectID: UUID,
        resourceID: String,
        attemptID: UUID,
        userIdeas: String
    ) {
        self.projectID = projectID
        self.resourceID = resourceID
        self.attemptID = attemptID
        self.userIdeas = userIdeas
    }
}

public struct StoryAssetImageGenerationEvent: Equatable, Sendable {
    public let project: StoryProject

    public init(project: StoryProject) {
        self.project = project
    }
}

public struct StoryAssetImageGenerationOutcome: Equatable, Sendable {
    public let project: StoryProject

    public init(project: StoryProject) {
        self.project = project
    }
}

public protocol StoryAssetImageGenerating: Sendable {
    func generateStoryAssetImage(
        ownerUserID: String,
        command: StoryAssetImageGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryAssetImageGenerationEvent) async -> Void
    ) async throws -> StoryAssetImageGenerationOutcome
}

public struct StoryAssetImageGenerationUseCase: Sendable {
    public static let maximumUserIdeasLength = 2_000

    private let generator: any StoryAssetImageGenerating

    public init(generator: any StoryAssetImageGenerating) {
        self.generator = generator
    }

    public func prepare(
        ownerUserID ownerInput: String,
        project: StoryProject,
        resourceID: String,
        userIdeas: String,
        attemptID: UUID = UUID()
    ) throws -> StoryAssetImageGenerationCommand {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryAssetImageGenerationError.invalidOwner }
        try project.validate()
        guard let resource = project.resource(id: resourceID) else {
            throw StoryAssetImageGenerationError.targetUnavailable
        }
        guard resource.imageGenerationAttemptID == nil else {
            throw StoryError.unresolvedSubmission
        }
        return .init(
            projectID: project.id,
            resourceID: resourceID,
            attemptID: attemptID,
            userIdeas: Self.normalizedUserIdeas(userIdeas)
        )
    }

    public func generate(
        ownerUserID ownerInput: String,
        command: StoryAssetImageGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryAssetImageGenerationEvent) async -> Void
    ) async throws -> StoryAssetImageGenerationOutcome {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryAssetImageGenerationError.invalidOwner }
        let normalized = StoryAssetImageGenerationCommand(
            projectID: command.projectID,
            resourceID: command.resourceID,
            attemptID: command.attemptID,
            userIdeas: Self.normalizedUserIdeas(command.userIdeas)
        )
        try await continuationCheck()
        let outcome = try await generator.generateStoryAssetImage(
            ownerUserID: owner,
            command: normalized,
            continuationCheck: continuationCheck,
            onEvent: { event in
                guard Self.acceptsIntent(event.project, command: normalized) else { return }
                await onEvent(event)
            }
        )
        try await continuationCheck()
        guard Self.acceptsCompletion(outcome.project, command: normalized) else {
            throw StoryAssetImageGenerationError.identityMismatch
        }
        return outcome
    }

    private static func acceptsIntent(
        _ project: StoryProject,
        command: StoryAssetImageGenerationCommand
    ) -> Bool {
        project.id == command.projectID
            && project.resource(id: command.resourceID)?.imageGenerationAttemptID
                == command.attemptID
    }

    private static func acceptsCompletion(
        _ project: StoryProject,
        command: StoryAssetImageGenerationCommand
    ) -> Bool {
        guard project.id == command.projectID,
              let resource = project.resource(id: command.resourceID),
              resource.imageGenerationAttemptID == nil else { return false }
        return resource.images.contains { $0.generationAttemptID == command.attemptID }
    }

    private static func normalizedUserIdeas(_ value: String) -> String {
        String(value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .prefix(maximumUserIdeasLength))
    }
}

public enum StoryAssetImageGenerationError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case targetUnavailable
    case identityMismatch

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "剧情素材图片生成缺少有效的账号上下文。"
        case .targetUnavailable:
            "剧情素材已不存在，请刷新后重试。"
        case .identityMismatch:
            "剧情素材图片返回了不属于当前项目、素材或生成请求的结果。"
        }
    }
}

public struct StoryAssetImageGenerationTarget: Hashable, Sendable {
    public let projectID: UUID
    public let resourceID: String

    public init(projectID: UUID, resourceID: String) {
        self.projectID = projectID
        self.resourceID = resourceID
    }
}

public struct StoryAssetImageGenerationToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let target: StoryAssetImageGenerationTarget
    public let attemptID: UUID
    public let requestID: UUID

    public init(
        ownerUserID: String,
        accountSessionID: UUID,
        target: StoryAssetImageGenerationTarget,
        attemptID: UUID,
        requestID: UUID
    ) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.target = target
        self.attemptID = attemptID
        self.requestID = requestID
    }
}

public struct StoryAssetImageGenerationState: Equatable, Sendable {
    public private(set) var activeTokens: [
        StoryAssetImageGenerationTarget: StoryAssetImageGenerationToken
    ] = [:]

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        command: StoryAssetImageGenerationCommand
    ) -> StoryAssetImageGenerationToken? {
        let target = StoryAssetImageGenerationTarget(
            projectID: command.projectID,
            resourceID: command.resourceID
        )
        guard activeTokens[target] == nil else { return nil }
        let token = StoryAssetImageGenerationToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            target: target,
            attemptID: command.attemptID,
            requestID: UUID()
        )
        activeTokens[target] = token
        return token
    }

    public func accepts(
        _ token: StoryAssetImageGenerationToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeTokens[token.target] == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: StoryAssetImageGenerationToken) -> Bool {
        guard activeTokens[token.target] == token else { return false }
        activeTokens[token.target] = nil
        return true
    }

    public mutating func reset() {
        activeTokens = [:]
    }
}
