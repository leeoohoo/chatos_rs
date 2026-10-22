import Foundation

public struct StoryFrameImageGenerationCommand: Equatable, Sendable {
    public let project: StoryProject
    public let segmentID: String
    public let role: StoryFrameRole
    public let referenceResourceIDs: [String]
    public let attemptID: UUID
    public let userIdeas: String

    public init(
        project: StoryProject,
        segmentID: String,
        role: StoryFrameRole,
        referenceResourceIDs: [String],
        attemptID: UUID,
        userIdeas: String
    ) {
        self.project = project
        self.segmentID = segmentID
        self.role = role
        self.referenceResourceIDs = referenceResourceIDs
        self.attemptID = attemptID
        self.userIdeas = userIdeas
    }

    public var projectID: UUID { project.id }
}

public struct StoryFrameImageGenerationEvent: Equatable, Sendable {
    public let project: StoryProject

    public init(project: StoryProject) {
        self.project = project
    }
}

public struct StoryFrameImageGenerationOutcome: Equatable, Sendable {
    public let project: StoryProject

    public init(project: StoryProject) {
        self.project = project
    }
}

public protocol StoryFrameImageGenerating: Sendable {
    func generateStoryFrameImage(
        ownerUserID: String,
        command: StoryFrameImageGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryFrameImageGenerationEvent) async -> Void
    ) async throws -> StoryFrameImageGenerationOutcome
}

public struct StoryFrameImageGenerationUseCase: Sendable {
    public static let maximumUserIdeasLength = 2_000

    private let generator: any StoryFrameImageGenerating

    public init(generator: any StoryFrameImageGenerating) {
        self.generator = generator
    }

    public func prepare(
        ownerUserID ownerInput: String,
        project: StoryProject,
        segmentID: String,
        role: StoryFrameRole,
        referenceResourceIDs requestedReferenceResourceIDs: [String],
        userIdeas: String,
        attemptID: UUID = UUID()
    ) throws -> StoryFrameImageGenerationCommand {
        _ = try Self.validateOwner(ownerInput)
        try project.validate()
        guard let segment = project.segments.first(where: { $0.id == segmentID }),
              segment.detail != nil,
              !segment.hasVideoJob else {
            throw StoryFrameImageGenerationError.targetUnavailable
        }
        let referenceResourceIDs = segment.resourceIDs.filter(
            requestedReferenceResourceIDs.contains
        )
        guard !referenceResourceIDs.isEmpty,
              Set(referenceResourceIDs) == Set(requestedReferenceResourceIDs),
              referenceResourceIDs.allSatisfy({
                  project.resource(id: $0)?.confirmedImage != nil
              }) else {
            throw StoryFrameImageGenerationError.invalidReferences
        }
        guard segment.frames(for: role).generationAttemptID == nil else {
            throw StoryError.unresolvedSubmission
        }
        return .init(
            project: project,
            segmentID: segmentID,
            role: role,
            referenceResourceIDs: referenceResourceIDs,
            attemptID: attemptID,
            userIdeas: Self.normalizedUserIdeas(userIdeas)
        )
    }

    public func generate(
        ownerUserID ownerInput: String,
        command: StoryFrameImageGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryFrameImageGenerationEvent) async -> Void
    ) async throws -> StoryFrameImageGenerationOutcome {
        let owner = try Self.validateOwner(ownerInput)
        let normalized = try normalized(command)
        try await continuationCheck()
        let outcome = try await generator.generateStoryFrameImage(
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
            throw StoryFrameImageGenerationError.identityMismatch
        }
        return outcome
    }

    private func normalized(
        _ command: StoryFrameImageGenerationCommand
    ) throws -> StoryFrameImageGenerationCommand {
        let prepared = try prepare(
            ownerUserID: "validated-owner",
            project: command.project,
            segmentID: command.segmentID,
            role: command.role,
            referenceResourceIDs: command.referenceResourceIDs,
            userIdeas: command.userIdeas,
            attemptID: command.attemptID
        )
        return prepared
    }

    private static func acceptsIntent(
        _ project: StoryProject,
        command: StoryFrameImageGenerationCommand
    ) -> Bool {
        guard project.id == command.projectID,
              let segment = project.segments.first(where: { $0.id == command.segmentID }) else {
            return false
        }
        return segment.frames(for: command.role).generationAttemptID == command.attemptID
    }

    private static func acceptsCompletion(
        _ project: StoryProject,
        command: StoryFrameImageGenerationCommand
    ) -> Bool {
        guard project.id == command.projectID,
              let segment = project.segments.first(where: { $0.id == command.segmentID }) else {
            return false
        }
        let frames = segment.frames(for: command.role)
        return frames.generationAttemptID == nil
            && frames.images.contains { $0.generationAttemptID == command.attemptID }
    }

    private static func validateOwner(_ input: String) throws -> String {
        let owner = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryFrameImageGenerationError.invalidOwner }
        return owner
    }

    private static func normalizedUserIdeas(_ value: String) -> String {
        String(value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .prefix(maximumUserIdeasLength))
    }
}

public enum StoryFrameImageGenerationError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case targetUnavailable
    case invalidReferences
    case identityMismatch

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "剧情首尾帧生成缺少有效的账号上下文。"
        case .targetUnavailable:
            "剧情分段已不存在或不再允许生成图片，请刷新后重试。"
        case .invalidReferences:
            "请选择至少一个属于当前分段且已确认图片的素材。"
        case .identityMismatch:
            "剧情首尾帧返回了不属于当前项目、分段、帧角色或生成请求的结果。"
        }
    }
}

public struct StoryFrameImageGenerationTarget: Equatable, Sendable {
    public let projectID: UUID
    public let segmentID: String
    public let role: StoryFrameRole

    public init(projectID: UUID, segmentID: String, role: StoryFrameRole) {
        self.projectID = projectID
        self.segmentID = segmentID
        self.role = role
    }
}

public struct StoryFrameImageGenerationToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let target: StoryFrameImageGenerationTarget
    public let attemptID: UUID
    public let requestID: UUID

    public init(
        ownerUserID: String,
        accountSessionID: UUID,
        target: StoryFrameImageGenerationTarget,
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

public struct StoryFrameImageGenerationState: Equatable, Sendable {
    public private(set) var activeToken: StoryFrameImageGenerationToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        command: StoryFrameImageGenerationCommand
    ) -> StoryFrameImageGenerationToken? {
        guard activeToken == nil else { return nil }
        let token = StoryFrameImageGenerationToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            target: .init(
                projectID: command.projectID,
                segmentID: command.segmentID,
                role: command.role
            ),
            attemptID: command.attemptID,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: StoryFrameImageGenerationToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: StoryFrameImageGenerationToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
