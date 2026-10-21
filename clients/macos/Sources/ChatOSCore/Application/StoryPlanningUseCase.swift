import CryptoKit
import Foundation

public enum StoryPlanningStage: String, Codable, Equatable, Sendable {
    case outline
    case refine
}

public enum StoryPlanningRunStatus: String, Codable, Equatable, Sendable {
    case ready
    case running
    case paused
    case completed
    case failed
    case needsReview
    case limitReached
}

public struct StoryPlanningRunEvent: Identifiable, Equatable, Sendable {
    public var id: UUID
    public var date: Date
    public var kind: String
    public var detail: String
    public var modelCalls: Int

    public init(
        id: UUID,
        date: Date,
        kind: String,
        detail: String,
        modelCalls: Int
    ) {
        self.id = id
        self.date = date
        self.kind = kind
        self.detail = detail
        self.modelCalls = modelCalls
    }
}

/// Safe application/UI projection of a persisted Story Agent run.
///
/// Provider messages, tool arguments, receipts, memory checkpoints, usage payloads,
/// model credentials, and runtime scope remain behind the platform adapter.
public struct StoryPlanningRunSnapshot: Identifiable, Equatable, Sendable {
    public var id: UUID
    public var ownerUserID: String
    public var projectID: UUID
    public var stage: StoryPlanningStage
    public var targetIDs: [String]
    public var baseProjectFingerprint: String
    public var draft: StoryProject
    public var events: [StoryPlanningRunEvent]
    public var status: StoryPlanningRunStatus
    public var stopReason: String?
    public var modelCalls: Int
    public var usesCloudMemory: Bool
    public var applied: Bool
    public var abandonedAt: Date?
    public var updatedAt: Date

    public init(
        id: UUID,
        ownerUserID: String,
        projectID: UUID,
        stage: StoryPlanningStage,
        targetIDs: [String],
        baseProjectFingerprint: String,
        draft: StoryProject,
        events: [StoryPlanningRunEvent],
        status: StoryPlanningRunStatus,
        stopReason: String?,
        modelCalls: Int,
        usesCloudMemory: Bool,
        applied: Bool,
        abandonedAt: Date?,
        updatedAt: Date
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.stage = stage
        self.targetIDs = targetIDs
        self.baseProjectFingerprint = baseProjectFingerprint
        self.draft = draft
        self.events = events
        self.status = status
        self.stopReason = stopReason
        self.modelCalls = modelCalls
        self.usesCloudMemory = usesCloudMemory
        self.applied = applied
        self.abandonedAt = abandonedAt
        self.updatedAt = updatedAt
    }

    public var canResume: Bool {
        !applied && abandonedAt == nil && status != .completed
    }
}

public enum StoryPlanningModelStreamEvent: Equatable, Sendable {
    case activity(bytes: Int)
    case responseCreated
    case textDelta(String)
    case toolCallDelta(name: String?)
    case completed
}

public enum StoryPlanningExecutionEvent: Equatable, Sendable {
    case run(StoryPlanningRunSnapshot)
    case modelStream(StoryPlanningModelStreamEvent)
}

public enum StoryPlanningCommand: Equatable, Sendable {
    case start(
        project: StoryProject,
        stage: StoryPlanningStage,
        targetIDs: [String],
        userIdeas: String
    )
    case resume(project: StoryProject, runID: UUID)
    case abandon(projectID: UUID, runID: UUID)

    public var projectID: UUID {
        switch self {
        case let .start(project, _, _, _), let .resume(project, _):
            project.id
        case let .abandon(projectID, _):
            projectID
        }
    }

    public var expectedRunID: UUID? {
        switch self {
        case .start:
            nil
        case let .resume(_, runID), let .abandon(_, runID):
            runID
        }
    }
}

public struct StoryPlanningExecutionOutcome: Equatable, Sendable {
    public var run: StoryPlanningRunSnapshot
    public var project: StoryProject?
    public var stopReason: String?

    public init(
        run: StoryPlanningRunSnapshot,
        project: StoryProject? = nil,
        stopReason: String? = nil
    ) {
        self.run = run
        self.project = project
        self.stopReason = stopReason
    }
}

public protocol StoryPlanningExecuting: Sendable {
    func validateStoryPlanningAvailability() throws

    func executeStoryPlanning(
        ownerUserID: String,
        command: StoryPlanningCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        onEvent: @escaping @Sendable (StoryPlanningExecutionEvent) async -> Void
    ) async throws -> StoryPlanningExecutionOutcome
}

public struct StoryPlanningUseCase: Sendable {
    public static let maximumUserIdeasLength = 2_000

    private let executor: any StoryPlanningExecuting

    public init(executor: any StoryPlanningExecuting) {
        self.executor = executor
    }

    public func validateAvailability() throws {
        try executor.validateStoryPlanningAvailability()
    }

    public func execute(
        ownerUserID ownerInput: String,
        command: StoryPlanningCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        onEvent: @escaping @Sendable (StoryPlanningExecutionEvent) async -> Void
    ) async throws -> StoryPlanningExecutionOutcome {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryPlanningError.invalidOwner }
        let normalized = try normalized(command)
        try await continuationCheck()
        let outcome = try await executor.executeStoryPlanning(
            ownerUserID: owner,
            command: normalized,
            continuationCheck: continuationCheck,
            shouldPause: shouldPause,
            onEvent: { event in
                switch event {
                case let .run(snapshot):
                    guard Self.accepts(
                        snapshot,
                        ownerUserID: owner,
                        command: normalized
                    ) else { return }
                    await onEvent(event)
                case .modelStream:
                    await onEvent(event)
                }
            }
        )
        try await continuationCheck()
        guard Self.accepts(
                outcome.run,
                ownerUserID: owner,
                command: normalized
              ),
              outcome.project?.id == normalized.projectID || outcome.project == nil else {
            throw StoryPlanningError.identityMismatch
        }
        return outcome
    }

    private static func accepts(
        _ snapshot: StoryPlanningRunSnapshot,
        ownerUserID: String,
        command: StoryPlanningCommand
    ) -> Bool {
        snapshot.ownerUserID == ownerUserID
            && snapshot.projectID == command.projectID
            && command.expectedRunID.map { snapshot.id == $0 } != false
    }

    private func normalized(_ command: StoryPlanningCommand) throws -> StoryPlanningCommand {
        switch command {
        case let .start(project, stage, targetIDs, userIdeas):
            try project.validate()
            return .start(
                project: project,
                stage: stage,
                targetIDs: targetIDs,
                userIdeas: String(userIdeas
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                    .prefix(Self.maximumUserIdeasLength))
            )
        case let .resume(project, runID):
            try project.validate()
            return .resume(project: project, runID: runID)
        case .abandon:
            return command
        }
    }
}

public enum StoryPlanningError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case identityMismatch

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "剧情规划缺少有效的账号上下文。"
        case .identityMismatch:
            "剧情规划返回了不属于当前项目或账号的结果。"
        }
    }
}

public enum StoryPlanningOperationKind: Equatable, Sendable {
    case start(projectID: UUID, stage: StoryPlanningStage)
    case resume(projectID: UUID, runID: UUID)
    case abandon(projectID: UUID, runID: UUID)
}

public struct StoryPlanningExecutionToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let requestID: UUID
    public let kind: StoryPlanningOperationKind

    public init(
        ownerUserID: String,
        accountSessionID: UUID,
        requestID: UUID,
        kind: StoryPlanningOperationKind
    ) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.requestID = requestID
        self.kind = kind
    }
}

public struct StoryPlanningExecutionState: Equatable, Sendable {
    public private(set) var activeToken: StoryPlanningExecutionToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        kind: StoryPlanningOperationKind
    ) -> StoryPlanningExecutionToken? {
        guard activeToken == nil else { return nil }
        let token = StoryPlanningExecutionToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            requestID: UUID(),
            kind: kind
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: StoryPlanningExecutionToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: StoryPlanningExecutionToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}

public enum StoryProjectFingerprint {
    public static func current(_ project: StoryProject) throws -> String {
        var value = project
        value.updatedAt = .distantPast
        value.models.supportedVideoDurations = nil
        return try digest(value)
    }

    public static func legacyIncludingCapabilities(_ project: StoryProject) throws -> String {
        var value = project
        value.updatedAt = .distantPast
        return try digest(value)
    }

    public static func matches(_ persisted: String, project: StoryProject) throws -> Bool {
        if persisted == (try current(project)) { return true }
        return persisted == (try legacyIncludingCapabilities(project))
    }

    private static func digest(_ project: StoryProject) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return SHA256.hash(data: try encoder.encode(project))
            .map { String(format: "%02x", $0) }
            .joined()
    }
}
