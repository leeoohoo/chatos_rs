import Foundation

public enum StoryMediaBatchExecutionMode: Equatable, Sendable {
    case persistentBatch
    case directVideos
}

public struct StoryMediaBatchPreparation: Equatable, Sendable {
    public var batchID: UUID
    public var ownerUserID: String
    public var projectID: UUID
    public var projectFingerprint: String
    public var kind: StoryMediaBatchKind
    public var requestedTargetIDs: [String]
    public var steps: [StoryMediaBatchStepSnapshot]
    public var executionMode: StoryMediaBatchExecutionMode

    public init(
        batchID: UUID,
        ownerUserID: String,
        projectID: UUID,
        projectFingerprint: String,
        kind: StoryMediaBatchKind,
        requestedTargetIDs: [String],
        steps: [StoryMediaBatchStepSnapshot],
        executionMode: StoryMediaBatchExecutionMode
    ) {
        self.batchID = batchID
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.projectFingerprint = projectFingerprint
        self.kind = kind
        self.requestedTargetIDs = requestedTargetIDs
        self.steps = steps
        self.executionMode = executionMode
    }
}

public enum StoryMediaBatchCommand: Equatable, Sendable {
    case start(
        project: StoryProject,
        preparation: StoryMediaBatchPreparation,
        availableModels: [MediaGenerationModel],
        userIdeas: String
    )
    case resume(projectID: UUID, batchID: UUID)
    case abandon(project: StoryProject, batchID: UUID)

    public var projectID: UUID {
        switch self {
        case let .start(project, _, _, _), let .abandon(project, _):
            project.id
        case let .resume(projectID, _):
            projectID
        }
    }

    public var batchID: UUID {
        switch self {
        case let .start(_, preparation, _, _):
            preparation.batchID
        case let .resume(_, batchID), let .abandon(_, batchID):
            batchID
        }
    }
}

public struct StoryMediaBatchExecutionEvent: Equatable, Sendable {
    public var batch: StoryMediaBatchSnapshot
    public var project: StoryProject

    public init(batch: StoryMediaBatchSnapshot, project: StoryProject) {
        self.batch = batch
        self.project = project
    }
}

public struct StoryMediaBatchExecutionOutcome: Equatable, Sendable {
    public var batch: StoryMediaBatchSnapshot
    public var project: StoryProject

    public init(batch: StoryMediaBatchSnapshot, project: StoryProject) {
        self.batch = batch
        self.project = project
    }
}

public protocol StoryMediaBatchExecuting: Sendable {
    func prepareStoryMediaBatch(
        ownerUserID: String,
        project: StoryProject,
        kind: StoryMediaBatchKind,
        targetIDs: [String],
        availableModels: [MediaGenerationModel],
        userIdeas: String
    ) throws -> StoryMediaBatchPreparation

    func executeStoryMediaBatch(
        ownerUserID: String,
        command: StoryMediaBatchCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        onEvent: @escaping @Sendable (StoryMediaBatchExecutionEvent) async -> Void
    ) async throws -> StoryMediaBatchExecutionOutcome
}

public struct StoryMediaBatchUseCase: Sendable {
    public static let maximumUserIdeasLength = 2_000

    private let executor: any StoryMediaBatchExecuting

    public init(executor: any StoryMediaBatchExecuting) {
        self.executor = executor
    }

    public func eligibleTargetIDs(
        project: StoryProject,
        kind: StoryMediaBatchKind
    ) -> [String] {
        switch kind {
        case .assets:
            project.resources.filter {
                $0.images.isEmpty && $0.imageGenerationAttemptID == nil
            }.map(\.id)
        case .frames:
            project.segments.filter { segment in
                segment.detail != nil
                    && segment.attempt == nil
                    && segment.video == nil
                    && segment.firstFrames.images.isEmpty
                    && segment.imageGenerationAttemptID == nil
                    && segment.resourceIDs.allSatisfy {
                        project.resource(id: $0)?.confirmedImage != nil
                    }
            }.map(\.id)
        case .lastFrames:
            project.segments.filter { segment in
                segment.detail != nil
                    && segment.attempt == nil
                    && segment.video == nil
                    && segment.lastFrames.images.isEmpty
                    && segment.lastFrameGenerationAttemptID == nil
                    && segment.resourceIDs.allSatisfy {
                        project.resource(id: $0)?.confirmedImage != nil
                    }
            }.map(\.id)
        case .videos:
            project.segments.filter(\.isReady).map(\.id)
        case .pipeline:
            project.segments.filter {
                $0.detail != nil
                    && $0.attempt == nil
                    && $0.video == nil
                    && $0.imageGenerationAttemptID == nil
                    && $0.lastFrameGenerationAttemptID == nil
            }.map(\.id)
        }
    }

    public func prepare(
        ownerUserID ownerInput: String,
        project: StoryProject,
        kind: StoryMediaBatchKind,
        targetIDs: [String],
        availableModels: [MediaGenerationModel],
        userIdeas: String
    ) throws -> StoryMediaBatchPreparation {
        let owner = try normalizedOwner(ownerInput)
        try project.validate()
        let normalizedIdeas = Self.normalizedUserIdeas(userIdeas)
        let preparation = try executor.prepareStoryMediaBatch(
            ownerUserID: owner,
            project: project,
            kind: kind,
            targetIDs: targetIDs,
            availableModels: availableModels,
            userIdeas: normalizedIdeas
        )
        guard preparation.ownerUserID == owner,
              preparation.projectID == project.id,
              preparation.projectFingerprint == (try StoryProjectFingerprint.current(project)),
              preparation.kind == kind,
              preparation.requestedTargetIDs == targetIDs,
              !preparation.steps.isEmpty,
              preparation.steps.count <= 500,
              Set(preparation.steps.map(\.id)).count == preparation.steps.count,
              preparation.steps.allSatisfy({ $0.kind != .pipeline }),
              (kind == .videos) == (preparation.executionMode == .directVideos)
        else { throw StoryMediaBatchUseCaseError.identityMismatch }
        return preparation
    }

    public func execute(
        ownerUserID ownerInput: String,
        command: StoryMediaBatchCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        onEvent: @escaping @Sendable (StoryMediaBatchExecutionEvent) async -> Void
    ) async throws -> StoryMediaBatchExecutionOutcome {
        let owner = try normalizedOwner(ownerInput)
        let normalized = try normalized(command, ownerUserID: owner)
        try await continuationCheck()
        let outcome = try await executor.executeStoryMediaBatch(
            ownerUserID: owner,
            command: normalized,
            continuationCheck: continuationCheck,
            shouldPause: shouldPause,
            onEvent: { event in
                guard Self.accepts(event, ownerUserID: owner, command: normalized) else {
                    return
                }
                await onEvent(event)
            }
        )
        try await continuationCheck()
        guard Self.accepts(
            .init(batch: outcome.batch, project: outcome.project),
            ownerUserID: owner,
            command: normalized
        ) else { throw StoryMediaBatchUseCaseError.identityMismatch }
        return outcome
    }

    private func normalizedOwner(_ input: String) throws -> String {
        let owner = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryMediaBatchUseCaseError.invalidOwner }
        return owner
    }

    private func normalized(
        _ command: StoryMediaBatchCommand,
        ownerUserID: String
    ) throws -> StoryMediaBatchCommand {
        switch command {
        case let .start(project, preparation, availableModels, userIdeas):
            try project.validate()
            guard preparation.ownerUserID == ownerUserID,
                  preparation.projectID == project.id,
                  preparation.projectFingerprint == (try StoryProjectFingerprint.current(project)),
                  preparation.executionMode == .persistentBatch,
                  preparation.kind != .videos else {
                throw StoryMediaBatchUseCaseError.directVideosRequireSeparateExecution
            }
            return .start(
                project: project,
                preparation: preparation,
                availableModels: availableModels,
                userIdeas: Self.normalizedUserIdeas(userIdeas)
            )
        case .resume:
            return command
        case let .abandon(project, batchID):
            try project.validate()
            return .abandon(project: project, batchID: batchID)
        }
    }

    private static func accepts(
        _ event: StoryMediaBatchExecutionEvent,
        ownerUserID: String,
        command: StoryMediaBatchCommand
    ) -> Bool {
        event.batch.ownerUserID == ownerUserID
            && event.batch.projectID == command.projectID
            && event.batch.id == command.batchID
            && event.project.id == command.projectID
    }

    private static func normalizedUserIdeas(_ value: String) -> String {
        String(value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .prefix(maximumUserIdeasLength))
    }
}

public enum StoryMediaBatchUseCaseError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case identityMismatch
    case directVideosRequireSeparateExecution

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "剧情制作批次缺少有效的账号上下文。"
        case .identityMismatch:
            "剧情制作批次返回了不属于当前账号、项目或操作的结果。"
        case .directVideosRequireSeparateExecution:
            "仅视频制作必须使用逐段视频生成队列。"
        }
    }
}

public enum StoryMediaBatchOperationKind: Equatable, Sendable {
    case start(projectID: UUID, batchID: UUID)
    case resume(projectID: UUID, batchID: UUID)
    case abandon(projectID: UUID, batchID: UUID)

    public var projectID: UUID {
        switch self {
        case let .start(projectID, _),
             let .resume(projectID, _),
             let .abandon(projectID, _):
            projectID
        }
    }

    public var batchID: UUID {
        switch self {
        case let .start(_, batchID),
             let .resume(_, batchID),
             let .abandon(_, batchID):
            batchID
        }
    }
}

public struct StoryMediaBatchExecutionToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let requestID: UUID
    public let kind: StoryMediaBatchOperationKind

    public init(
        ownerUserID: String,
        accountSessionID: UUID,
        requestID: UUID,
        kind: StoryMediaBatchOperationKind
    ) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.requestID = requestID
        self.kind = kind
    }
}

public struct StoryMediaBatchExecutionState: Equatable, Sendable {
    public private(set) var activeToken: StoryMediaBatchExecutionToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        kind: StoryMediaBatchOperationKind
    ) -> StoryMediaBatchExecutionToken? {
        guard activeToken == nil else { return nil }
        let token = StoryMediaBatchExecutionToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            requestID: UUID(),
            kind: kind
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: StoryMediaBatchExecutionToken,
        ownerUserID: String?,
        accountSessionID: UUID,
        projectID: UUID?
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
            && token.kind.projectID == projectID
    }

    @discardableResult
    public mutating func finish(_ token: StoryMediaBatchExecutionToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
