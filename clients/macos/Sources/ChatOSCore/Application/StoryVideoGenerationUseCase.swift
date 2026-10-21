import Foundation

public enum StoryVideoGenerationKind: Equatable, Sendable {
    case start(model: MediaGenerationModel)
    case resume(jobID: String)
}

public struct StoryVideoGenerationCommand: Equatable, Sendable {
    public let project: StoryProject
    public let segmentID: String
    public let attempt: StoryVideoAttempt
    public let kind: StoryVideoGenerationKind

    public init(
        project: StoryProject,
        segmentID: String,
        attempt: StoryVideoAttempt,
        kind: StoryVideoGenerationKind
    ) {
        self.project = project
        self.segmentID = segmentID
        self.attempt = attempt
        self.kind = kind
    }

    public var projectID: UUID { project.id }
}

public struct StoryVideoGenerationProgress: Equatable, Sendable {
    public let status: String
    public let percent: Double?

    public init(status: String, percent: Double? = nil) {
        self.status = status
        self.percent = percent
    }
}

public struct StoryVideoGenerationEvent: Equatable, Sendable {
    public let project: StoryProject
    public let progress: StoryVideoGenerationProgress
    public let error: String?

    public init(
        project: StoryProject,
        progress: StoryVideoGenerationProgress,
        error: String? = nil
    ) {
        self.project = project
        self.progress = progress
        self.error = error
    }
}

public enum StoryVideoGenerationOutcomeStatus: Equatable, Sendable {
    case completed
    case failedRetainingAttempt
    case failedBeforeSubmission
}

public struct StoryVideoGenerationOutcome: Equatable, Sendable {
    public let project: StoryProject
    public let status: StoryVideoGenerationOutcomeStatus

    public init(project: StoryProject, status: StoryVideoGenerationOutcomeStatus) {
        self.project = project
        self.status = status
    }
}

public protocol StoryVideoGenerating: Sendable {
    func prepareStoryVideoGeneration(
        ownerUserID: String,
        project: StoryProject,
        segmentID: String,
        model: MediaGenerationModel,
        size: String,
        userIdeas: String
    ) throws -> StoryVideoGenerationCommand

    func executeStoryVideoGeneration(
        ownerUserID: String,
        command: StoryVideoGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryVideoGenerationEvent) async -> Void
    ) async throws -> StoryVideoGenerationOutcome
}

public struct StoryVideoGenerationUseCase: Sendable {
    public static let maximumUserIdeasLength = 1_000

    private let generator: any StoryVideoGenerating

    public init(generator: any StoryVideoGenerating) {
        self.generator = generator
    }

    public func prepareStart(
        ownerUserID ownerInput: String,
        project: StoryProject,
        segmentID: String,
        model: MediaGenerationModel,
        size: String,
        userIdeas: String
    ) throws -> StoryVideoGenerationCommand {
        let owner = try Self.normalizedOwner(ownerInput)
        try project.validate()
        guard let segment = project.segments.first(where: { $0.id == segmentID }),
              segment.isReady,
              model.id == project.models.videoModelID,
              model.enabled,
              model.hasAPIKey else { throw StoryVideoGenerationError.targetUnavailable }
        let profile = VideoGenerationProfile(modelName: model.modelName)
        guard profile.durations.contains(segment.seconds), profile.sizes.contains(size) else {
            throw StoryError.unsupportedDuration
        }
        let command = try generator.prepareStoryVideoGeneration(
            ownerUserID: owner,
            project: project,
            segmentID: segmentID,
            model: model,
            size: size,
            userIdeas: Self.normalizedUserIdeas(userIdeas)
        )
        guard command.project == project,
              command.segmentID == segmentID,
              command.attempt.modelConfigID == project.models.videoModelID,
              command.attempt.size == size,
              command.attempt.ratio == project.ratio,
              command.attempt.seconds == segment.seconds,
              !command.attempt.prompt.isEmpty,
              case .start(let preparedModel) = command.kind,
              preparedModel == model else { throw StoryVideoGenerationError.identityMismatch }
        return command
    }

    public func prepareResume(
        ownerUserID ownerInput: String,
        project: StoryProject,
        segmentID: String
    ) throws -> StoryVideoGenerationCommand {
        _ = try Self.normalizedOwner(ownerInput)
        try project.validate()
        guard let segment = project.segments.first(where: { $0.id == segmentID }),
              segment.video == nil,
              let attempt = segment.attempt else {
            throw StoryVideoGenerationError.targetUnavailable
        }
        guard let jobID = attempt.jobID, !jobID.isEmpty else {
            throw StoryError.unresolvedSubmission
        }
        return .init(
            project: project,
            segmentID: segmentID,
            attempt: attempt,
            kind: .resume(jobID: jobID)
        )
    }

    public func execute(
        ownerUserID ownerInput: String,
        command: StoryVideoGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryVideoGenerationEvent) async -> Void
    ) async throws -> StoryVideoGenerationOutcome {
        let owner = try Self.normalizedOwner(ownerInput)
        try Self.validate(command)
        try await continuationCheck()
        let outcome = try await generator.executeStoryVideoGeneration(
            ownerUserID: owner,
            command: command,
            continuationCheck: continuationCheck,
            onEvent: { event in
                guard Self.acceptsActive(event.project, command: command) else { return }
                await onEvent(event)
            }
        )
        try await continuationCheck()
        guard Self.accepts(outcome, command: command) else {
            throw StoryVideoGenerationError.identityMismatch
        }
        return outcome
    }

    private static func validate(_ command: StoryVideoGenerationCommand) throws {
        try command.project.validate()
        guard let segment = command.project.segments.first(where: {
            $0.id == command.segmentID
        }) else { throw StoryVideoGenerationError.targetUnavailable }
        switch command.kind {
        case .start:
            guard segment.isReady else { throw StoryVideoGenerationError.targetUnavailable }
        case let .resume(jobID):
            guard segment.video == nil,
                  segment.attempt == command.attempt,
                  command.attempt.jobID == jobID else {
                throw StoryVideoGenerationError.identityMismatch
            }
        }
    }

    private static func acceptsActive(
        _ project: StoryProject,
        command: StoryVideoGenerationCommand
    ) -> Bool {
        project.id == command.projectID
            && project.segments.first(where: { $0.id == command.segmentID })?.attempt?.id
                == command.attempt.id
    }

    private static func accepts(
        _ outcome: StoryVideoGenerationOutcome,
        command: StoryVideoGenerationCommand
    ) -> Bool {
        guard outcome.project.id == command.projectID,
              let segment = outcome.project.segments.first(where: {
                $0.id == command.segmentID
              }) else { return false }
        switch outcome.status {
        case .completed:
            return segment.attempt?.id == command.attempt.id && segment.video != nil
        case .failedRetainingAttempt:
            return segment.attempt?.id == command.attempt.id
                && segment.video == nil && segment.error != nil
        case .failedBeforeSubmission:
            return segment.attempt == nil && segment.video == nil && segment.error != nil
        }
    }

    private static func normalizedOwner(_ input: String) throws -> String {
        let owner = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryVideoGenerationError.invalidOwner }
        return owner
    }

    private static func normalizedUserIdeas(_ value: String) -> String {
        String(value.trimmingCharacters(in: .whitespacesAndNewlines)
            .prefix(maximumUserIdeasLength))
    }
}

public enum StoryVideoGenerationError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case targetUnavailable
    case identityMismatch

    public var errorDescription: String? {
        switch self {
        case .invalidOwner: "剧情视频生成缺少有效的账号上下文。"
        case .targetUnavailable: "剧情分段已不存在或不再满足视频生成条件。"
        case .identityMismatch: "剧情视频返回了不属于当前项目、分段或生成请求的结果。"
        }
    }
}

public struct StoryVideoGenerationTarget: Hashable, Sendable {
    public let projectID: UUID
    public let segmentID: String

    public init(projectID: UUID, segmentID: String) {
        self.projectID = projectID
        self.segmentID = segmentID
    }
}

public struct StoryVideoGenerationToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let target: StoryVideoGenerationTarget
    public let attemptID: UUID
    public let requestID: UUID
}

public struct StoryVideoGenerationState: Equatable, Sendable {
    public private(set) var activeTokens: [StoryVideoGenerationTarget: StoryVideoGenerationToken] = [:]

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        command: StoryVideoGenerationCommand
    ) -> StoryVideoGenerationToken? {
        let target = StoryVideoGenerationTarget(
            projectID: command.projectID,
            segmentID: command.segmentID
        )
        guard activeTokens[target] == nil else { return nil }
        let token = StoryVideoGenerationToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            target: target,
            attemptID: command.attempt.id,
            requestID: UUID()
        )
        activeTokens[target] = token
        return token
    }

    public func accepts(
        _ token: StoryVideoGenerationToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeTokens[token.target] == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: StoryVideoGenerationToken) -> Bool {
        guard activeTokens[token.target] == token else { return false }
        activeTokens[token.target] = nil
        return true
    }

    public mutating func reset() { activeTokens = [:] }
}
