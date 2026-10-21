import Foundation

public struct StoryProjectCatalogCursor: Equatable, Sendable {
    public var modifiedAt: Date
    public var recordID: String

    public init(modifiedAt: Date, recordID: String) {
        self.modifiedAt = modifiedAt
        self.recordID = recordID
    }
}

public struct StoryProjectCatalogPage: Equatable, Sendable {
    public var projects: [StoryProject]
    public var unreadableCount: Int
    public var nextCursor: StoryProjectCatalogCursor?

    public init(
        projects: [StoryProject],
        unreadableCount: Int = 0,
        nextCursor: StoryProjectCatalogCursor? = nil
    ) {
        self.projects = projects
        self.unreadableCount = unreadableCount
        self.nextCursor = nextCursor
    }
}

public struct StoryProjectCatalogCollection: Equatable, Sendable {
    public var projects: [StoryProject]
    public var unreadableCount: Int
    public var nextCursor: StoryProjectCatalogCursor?

    public init(
        projects: [StoryProject] = [],
        unreadableCount: Int = 0,
        nextCursor: StoryProjectCatalogCursor? = nil
    ) {
        self.projects = projects
        self.unreadableCount = unreadableCount
        self.nextCursor = nextCursor
    }
}

public protocol StoryProjectCatalogLoading: Sendable {
    func loadStoryProjects(
        ownerUserID: String,
        after cursor: StoryProjectCatalogCursor?,
        limit: Int
    ) async throws -> StoryProjectCatalogPage
}

public struct StoryProjectCatalogUseCase: Sendable {
    public static let defaultPageSize = 40

    private let catalog: any StoryProjectCatalogLoading
    private let pageSize: Int

    public init(
        catalog: any StoryProjectCatalogLoading,
        pageSize: Int = Self.defaultPageSize
    ) {
        self.catalog = catalog
        self.pageSize = min(max(pageSize, 1), 200)
    }

    public func loadInitial(ownerUserID ownerInput: String) async throws -> StoryProjectCatalogPage {
        try await load(ownerUserID: ownerInput, after: nil)
    }

    public func loadNext(
        ownerUserID ownerInput: String,
        after cursor: StoryProjectCatalogCursor
    ) async throws -> StoryProjectCatalogPage {
        try await load(ownerUserID: ownerInput, after: cursor)
    }

    public func replacing(with page: StoryProjectCatalogPage) -> StoryProjectCatalogCollection {
        normalized(
            projects: page.projects,
            unreadableCount: page.unreadableCount,
            nextCursor: page.nextCursor
        )
    }

    public func merging(
        _ page: StoryProjectCatalogPage,
        into current: StoryProjectCatalogCollection
    ) -> StoryProjectCatalogCollection {
        let existing = Set(current.projects.map(\.id))
        return normalized(
            projects: current.projects + page.projects.filter { !existing.contains($0.id) },
            unreadableCount: current.unreadableCount + page.unreadableCount,
            nextCursor: page.nextCursor
        )
    }

    private func load(
        ownerUserID ownerInput: String,
        after cursor: StoryProjectCatalogCursor?
    ) async throws -> StoryProjectCatalogPage {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryProjectCatalogError.invalidOwner }
        let page = try await catalog.loadStoryProjects(
            ownerUserID: owner,
            after: cursor,
            limit: pageSize
        )
        try Task.checkCancellation()
        return page
    }

    private func normalized(
        projects: [StoryProject],
        unreadableCount: Int,
        nextCursor: StoryProjectCatalogCursor?
    ) -> StoryProjectCatalogCollection {
        var seen = Set<UUID>()
        return StoryProjectCatalogCollection(
            projects: projects
                .sorted { left, right in
                    left.updatedAt != right.updatedAt
                        ? left.updatedAt > right.updatedAt
                        : left.id.uuidString > right.id.uuidString
                }
                .filter { seen.insert($0.id).inserted },
            unreadableCount: max(0, unreadableCount),
            nextCursor: nextCursor
        )
    }
}

public enum StoryProjectCatalogError: LocalizedError, Equatable, Sendable {
    case invalidOwner

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "剧情项目缺少有效的账号上下文。"
        }
    }
}

public enum StoryProjectCatalogLoadKind: Equatable, Sendable {
    case initial
    case next(StoryProjectCatalogCursor)
}

public struct StoryProjectCatalogLoadToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let requestID: UUID
    public let kind: StoryProjectCatalogLoadKind

    public init(
        ownerUserID: String,
        accountSessionID: UUID,
        requestID: UUID,
        kind: StoryProjectCatalogLoadKind
    ) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.requestID = requestID
        self.kind = kind
    }
}

public struct StoryProjectCatalogLoadState: Equatable, Sendable {
    public private(set) var activeToken: StoryProjectCatalogLoadToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        kind: StoryProjectCatalogLoadKind
    ) -> StoryProjectCatalogLoadToken {
        let token = StoryProjectCatalogLoadToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            requestID: UUID(),
            kind: kind
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: StoryProjectCatalogLoadToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: StoryProjectCatalogLoadToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
