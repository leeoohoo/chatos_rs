import Foundation

public struct StoryHistoryCursor: Equatable, Sendable {
    public var modifiedAt: Date
    public var recordID: String

    public init(modifiedAt: Date, recordID: String) {
        self.modifiedAt = modifiedAt
        self.recordID = recordID
    }
}

public enum StoryMediaBatchKind: String, Codable, Equatable, Sendable {
    case assets
    case frames
    case lastFrames
    case videos
    case pipeline
}

public enum StoryMediaBatchStatus: String, Codable, Equatable, Sendable {
    case ready
    case running
    case paused
    case needsReview
    case completed
    case abandoned
}

public struct StoryMediaBatchStepSnapshot: Identifiable, Equatable, Sendable {
    public var id: String
    public var kind: StoryMediaBatchKind
    public var targetID: String
    public var completed: Bool
    public var progressStatus: String?

    public init(
        id: String,
        kind: StoryMediaBatchKind,
        targetID: String,
        completed: Bool,
        progressStatus: String? = nil
    ) {
        self.id = id
        self.kind = kind
        self.targetID = targetID
        self.completed = completed
        self.progressStatus = progressStatus
    }
}

public struct StoryMediaBatchEventSnapshot: Identifiable, Equatable, Sendable {
    public var id: UUID
    public var date: Date
    public var detail: String

    public init(id: UUID, date: Date, detail: String) {
        self.id = id
        self.date = date
        self.detail = detail
    }
}

/// Safe application/UI projection of a persisted production batch.
///
/// Provider job identities, generation intents, model configurations, persistence digests,
/// credentials, and local media filenames remain behind the platform adapter.
public struct StoryMediaBatchSnapshot: Identifiable, Equatable, Sendable {
    public var id: UUID
    public var ownerUserID: String
    public var projectID: UUID
    public var kind: StoryMediaBatchKind
    public var status: StoryMediaBatchStatus
    public var steps: [StoryMediaBatchStepSnapshot]
    public var events: [StoryMediaBatchEventSnapshot]
    public var consentAt: Date
    public var error: String?
    public var updatedAt: Date

    public init(
        id: UUID,
        ownerUserID: String,
        projectID: UUID,
        kind: StoryMediaBatchKind,
        status: StoryMediaBatchStatus,
        steps: [StoryMediaBatchStepSnapshot],
        events: [StoryMediaBatchEventSnapshot],
        consentAt: Date,
        error: String?,
        updatedAt: Date
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.kind = kind
        self.status = status
        self.steps = steps
        self.events = events
        self.consentAt = consentAt
        self.error = error
        self.updatedAt = updatedAt
    }

    public var finished: Bool {
        status == .completed || status == .abandoned
    }

    public var completedCount: Int {
        steps.lazy.filter(\.completed).count
    }
}

public struct StoryPlanningRunHistoryPage: Equatable, Sendable {
    public var runs: [StoryPlanningRunSnapshot]
    public var unreadableCount: Int
    public var nextCursor: StoryHistoryCursor?
    public var recoveredProject: StoryProject?

    public init(
        runs: [StoryPlanningRunSnapshot],
        unreadableCount: Int = 0,
        nextCursor: StoryHistoryCursor? = nil,
        recoveredProject: StoryProject? = nil
    ) {
        self.runs = runs
        self.unreadableCount = unreadableCount
        self.nextCursor = nextCursor
        self.recoveredProject = recoveredProject
    }
}

public struct StoryMediaBatchHistoryPage: Equatable, Sendable {
    public var batches: [StoryMediaBatchSnapshot]
    public var unreadableCount: Int
    public var nextCursor: StoryHistoryCursor?

    public init(
        batches: [StoryMediaBatchSnapshot],
        unreadableCount: Int = 0,
        nextCursor: StoryHistoryCursor? = nil
    ) {
        self.batches = batches
        self.unreadableCount = unreadableCount
        self.nextCursor = nextCursor
    }
}

public struct StoryHistoryInitialPage: Equatable, Sendable {
    public var planningRuns: StoryPlanningRunHistoryPage
    public var mediaBatches: StoryMediaBatchHistoryPage

    public init(
        planningRuns: StoryPlanningRunHistoryPage,
        mediaBatches: StoryMediaBatchHistoryPage
    ) {
        self.planningRuns = planningRuns
        self.mediaBatches = mediaBatches
    }
}

public protocol StoryHistoryLoading: Sendable {
    func loadStoryPlanningRuns(
        ownerUserID: String,
        projectID: UUID,
        currentProject: StoryProject?,
        after cursor: StoryHistoryCursor?,
        limit: Int
    ) async throws -> StoryPlanningRunHistoryPage

    func loadStoryMediaBatches(
        ownerUserID: String,
        projectID: UUID,
        after cursor: StoryHistoryCursor?,
        limit: Int
    ) async throws -> StoryMediaBatchHistoryPage
}

public struct StoryHistoryUseCase: Sendable {
    public static let defaultPageSize = 20

    private let history: any StoryHistoryLoading
    private let pageSize: Int

    public init(
        history: any StoryHistoryLoading,
        pageSize: Int = Self.defaultPageSize
    ) {
        self.history = history
        self.pageSize = min(max(pageSize, 1), 200)
    }

    public func loadInitial(
        ownerUserID ownerInput: String,
        projectID: UUID,
        currentProject: StoryProject?
    ) async throws -> StoryHistoryInitialPage {
        let owner = try normalizedOwner(ownerInput)
        if let currentProject {
            guard currentProject.id == projectID else {
                throw StoryHistoryError.identityMismatch
            }
            try currentProject.validate()
        }
        let runs = try await history.loadStoryPlanningRuns(
            ownerUserID: owner,
            projectID: projectID,
            currentProject: currentProject,
            after: nil,
            limit: pageSize
        )
        try Task.checkCancellation()
        let normalizedRuns = try normalized(runs, ownerUserID: owner, projectID: projectID)
        let batches = try await history.loadStoryMediaBatches(
            ownerUserID: owner,
            projectID: projectID,
            after: nil,
            limit: pageSize
        )
        try Task.checkCancellation()
        return StoryHistoryInitialPage(
            planningRuns: normalizedRuns,
            mediaBatches: try normalized(batches, ownerUserID: owner, projectID: projectID)
        )
    }

    public func loadMorePlanningRuns(
        ownerUserID ownerInput: String,
        projectID: UUID,
        after cursor: StoryHistoryCursor
    ) async throws -> StoryPlanningRunHistoryPage {
        let owner = try normalizedOwner(ownerInput)
        let page = try await history.loadStoryPlanningRuns(
            ownerUserID: owner,
            projectID: projectID,
            currentProject: nil,
            after: cursor,
            limit: pageSize
        )
        try Task.checkCancellation()
        return try normalized(page, ownerUserID: owner, projectID: projectID)
    }

    public func loadMoreMediaBatches(
        ownerUserID ownerInput: String,
        projectID: UUID,
        after cursor: StoryHistoryCursor
    ) async throws -> StoryMediaBatchHistoryPage {
        let owner = try normalizedOwner(ownerInput)
        let page = try await history.loadStoryMediaBatches(
            ownerUserID: owner,
            projectID: projectID,
            after: cursor,
            limit: pageSize
        )
        try Task.checkCancellation()
        return try normalized(page, ownerUserID: owner, projectID: projectID)
    }

    private func normalizedOwner(_ input: String) throws -> String {
        let owner = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryHistoryError.invalidOwner }
        return owner
    }

    private func normalized(
        _ page: StoryPlanningRunHistoryPage,
        ownerUserID: String,
        projectID: UUID
    ) throws -> StoryPlanningRunHistoryPage {
        guard page.runs.allSatisfy({
            $0.ownerUserID == ownerUserID
                && $0.projectID == projectID
                && $0.draft.id == projectID
        }), page.recoveredProject?.id == projectID || page.recoveredProject == nil,
        Self.valid(page.nextCursor) else {
            throw StoryHistoryError.identityMismatch
        }
        if let recoveredProject = page.recoveredProject {
            try recoveredProject.validate()
        }
        var seen = Set<UUID>()
        return StoryPlanningRunHistoryPage(
            runs: page.runs
                .sorted(by: Self.newerRun)
                .filter { seen.insert($0.id).inserted },
            unreadableCount: max(0, page.unreadableCount),
            nextCursor: page.nextCursor,
            recoveredProject: page.recoveredProject
        )
    }

    private func normalized(
        _ page: StoryMediaBatchHistoryPage,
        ownerUserID: String,
        projectID: UUID
    ) throws -> StoryMediaBatchHistoryPage {
        guard page.batches.allSatisfy({
            $0.ownerUserID == ownerUserID
                && $0.projectID == projectID
                && !$0.steps.isEmpty
                && $0.steps.count <= 500
                && $0.events.count <= 20_000
        }), Self.valid(page.nextCursor) else {
            throw StoryHistoryError.identityMismatch
        }
        var seen = Set<UUID>()
        return StoryMediaBatchHistoryPage(
            batches: page.batches
                .sorted(by: Self.newerBatch)
                .filter { seen.insert($0.id).inserted },
            unreadableCount: max(0, page.unreadableCount),
            nextCursor: page.nextCursor
        )
    }

    private static func valid(_ cursor: StoryHistoryCursor?) -> Bool {
        cursor.map { !$0.recordID.isEmpty } ?? true
    }

    private static func newerRun(
        _ left: StoryPlanningRunSnapshot,
        _ right: StoryPlanningRunSnapshot
    ) -> Bool {
        left.updatedAt != right.updatedAt
            ? left.updatedAt > right.updatedAt
            : left.id.uuidString > right.id.uuidString
    }

    private static func newerBatch(
        _ left: StoryMediaBatchSnapshot,
        _ right: StoryMediaBatchSnapshot
    ) -> Bool {
        left.updatedAt != right.updatedAt
            ? left.updatedAt > right.updatedAt
            : left.id.uuidString > right.id.uuidString
    }
}

public enum StoryHistoryError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case identityMismatch

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "剧情历史缺少有效的账号上下文。"
        case .identityMismatch:
            "剧情历史返回了不属于当前账号或项目的记录。"
        }
    }
}

public enum StoryHistoryLoadLane: Hashable, Sendable {
    case initial
    case planningRuns
    case mediaBatches
}

public enum StoryHistoryLoadKind: Equatable, Sendable {
    case initial(projectID: UUID)
    case planningRuns(projectID: UUID, cursor: StoryHistoryCursor)
    case mediaBatches(projectID: UUID, cursor: StoryHistoryCursor)

    public var projectID: UUID {
        switch self {
        case let .initial(projectID),
             let .planningRuns(projectID, _),
             let .mediaBatches(projectID, _):
            projectID
        }
    }

    public var lane: StoryHistoryLoadLane {
        switch self {
        case .initial: .initial
        case .planningRuns: .planningRuns
        case .mediaBatches: .mediaBatches
        }
    }
}

public struct StoryHistoryLoadToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let requestID: UUID
    public let kind: StoryHistoryLoadKind

    public init(
        ownerUserID: String,
        accountSessionID: UUID,
        requestID: UUID,
        kind: StoryHistoryLoadKind
    ) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.requestID = requestID
        self.kind = kind
    }
}

public struct StoryHistoryLoadState: Equatable, Sendable {
    public private(set) var activeTokens: [StoryHistoryLoadLane: StoryHistoryLoadToken] = [:]

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        kind: StoryHistoryLoadKind
    ) -> StoryHistoryLoadToken {
        if kind.lane == .initial {
            activeTokens.removeAll()
        }
        let token = StoryHistoryLoadToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            requestID: UUID(),
            kind: kind
        )
        activeTokens[kind.lane] = token
        return token
    }

    public func accepts(
        _ token: StoryHistoryLoadToken,
        ownerUserID: String?,
        accountSessionID: UUID,
        projectID: UUID?
    ) -> Bool {
        activeTokens[token.kind.lane] == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
            && token.kind.projectID == projectID
    }

    @discardableResult
    public mutating func finish(_ token: StoryHistoryLoadToken) -> Bool {
        guard activeTokens[token.kind.lane] == token else { return false }
        activeTokens[token.kind.lane] = nil
        return true
    }

    public mutating func reset() {
        activeTokens.removeAll()
    }
}
