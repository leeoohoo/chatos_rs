import Foundation

public struct MediaStudioVideoHistoryItem: Identifiable, Equatable, Sendable {
    public var id: String
    public var prompt: String
    public var modelName: String
    public var createdAt: Date
    public var fileURL: URL

    public init(
        id: String,
        prompt: String,
        modelName: String,
        createdAt: Date,
        fileURL: URL
    ) {
        self.id = id
        self.prompt = prompt
        self.modelName = modelName
        self.createdAt = createdAt
        self.fileURL = fileURL
    }
}

public struct MediaStudioHistoryCursor: Equatable, Sendable {
    public var modifiedAt: Date
    public var recordID: String

    public init(modifiedAt: Date, recordID: String) {
        self.modifiedAt = modifiedAt
        self.recordID = recordID
    }
}

public struct MediaStudioHistoryPage: Equatable, Sendable {
    public var images: [MediaStudioImageHistoryItem]
    public var videos: [MediaStudioVideoHistoryItem]
    public var unreadableCount: Int
    public var nextCursor: MediaStudioHistoryCursor?

    public init(
        images: [MediaStudioImageHistoryItem] = [],
        videos: [MediaStudioVideoHistoryItem] = [],
        unreadableCount: Int = 0,
        nextCursor: MediaStudioHistoryCursor? = nil
    ) {
        self.images = images
        self.videos = videos
        self.unreadableCount = unreadableCount
        self.nextCursor = nextCursor
    }
}

public struct MediaStudioHistoryCollection: Equatable, Sendable {
    public var images: [MediaStudioImageHistoryItem]
    public var videos: [MediaStudioVideoHistoryItem]
    public var unreadableCount: Int
    public var nextCursor: MediaStudioHistoryCursor?

    public init(
        images: [MediaStudioImageHistoryItem] = [],
        videos: [MediaStudioVideoHistoryItem] = [],
        unreadableCount: Int = 0,
        nextCursor: MediaStudioHistoryCursor? = nil
    ) {
        self.images = images
        self.videos = videos
        self.unreadableCount = unreadableCount
        self.nextCursor = nextCursor
    }
}

public protocol MediaStudioHistoryLoading: Sendable {
    func load(
        owner: String,
        after cursor: MediaStudioHistoryCursor?,
        limit: Int
    ) async throws -> MediaStudioHistoryPage
}

public struct MediaStudioHistoryUseCase: Sendable {
    public static let defaultPageSize = 60

    private let history: any MediaStudioHistoryLoading
    private let pageSize: Int

    public init(
        history: any MediaStudioHistoryLoading,
        pageSize: Int = Self.defaultPageSize
    ) {
        self.history = history
        self.pageSize = min(max(pageSize, 1), 200)
    }

    public func loadInitial(owner ownerInput: String) async throws -> MediaStudioHistoryPage {
        try await load(owner: ownerInput, after: nil)
    }

    public func loadNext(
        owner ownerInput: String,
        after cursor: MediaStudioHistoryCursor
    ) async throws -> MediaStudioHistoryPage {
        try await load(owner: ownerInput, after: cursor)
    }

    public func replacing(with page: MediaStudioHistoryPage) -> MediaStudioHistoryCollection {
        normalized(
            images: page.images,
            videos: page.videos,
            unreadableCount: page.unreadableCount,
            nextCursor: page.nextCursor
        )
    }

    public func merging(
        _ page: MediaStudioHistoryPage,
        into current: MediaStudioHistoryCollection
    ) -> MediaStudioHistoryCollection {
        let imageIDs = Set(current.images.map(\.id))
        let videoIDs = Set(current.videos.map(\.id))
        return normalized(
            images: current.images + page.images.filter { !imageIDs.contains($0.id) },
            videos: current.videos + page.videos.filter { !videoIDs.contains($0.id) },
            unreadableCount: current.unreadableCount + page.unreadableCount,
            nextCursor: page.nextCursor
        )
    }

    private func load(
        owner ownerInput: String,
        after cursor: MediaStudioHistoryCursor?
    ) async throws -> MediaStudioHistoryPage {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw MediaStudioHistoryError.invalidOwner }
        let page = try await history.load(owner: owner, after: cursor, limit: pageSize)
        try Task.checkCancellation()
        return page
    }

    private func normalized(
        images: [MediaStudioImageHistoryItem],
        videos: [MediaStudioVideoHistoryItem],
        unreadableCount: Int,
        nextCursor: MediaStudioHistoryCursor?
    ) -> MediaStudioHistoryCollection {
        MediaStudioHistoryCollection(
            images: images.sorted { $0.createdAt > $1.createdAt },
            videos: videos.sorted { $0.createdAt > $1.createdAt },
            unreadableCount: max(0, unreadableCount),
            nextCursor: nextCursor
        )
    }
}

public enum MediaStudioHistoryError: LocalizedError, Equatable, Sendable {
    case invalidOwner

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "创作记录缺少有效的账号上下文。"
        }
    }
}

public enum MediaStudioHistoryLoadKind: Equatable, Sendable {
    case initial
    case next(MediaStudioHistoryCursor)
}

public struct MediaStudioHistoryLoadToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let requestID: UUID
    public let kind: MediaStudioHistoryLoadKind

    public init(
        ownerUserID: String,
        accountSessionID: UUID,
        requestID: UUID,
        kind: MediaStudioHistoryLoadKind
    ) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.requestID = requestID
        self.kind = kind
    }
}

public struct MediaStudioHistoryLoadState: Equatable, Sendable {
    public private(set) var activeToken: MediaStudioHistoryLoadToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        kind: MediaStudioHistoryLoadKind
    ) -> MediaStudioHistoryLoadToken {
        let token = MediaStudioHistoryLoadToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            requestID: UUID(),
            kind: kind
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: MediaStudioHistoryLoadToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: MediaStudioHistoryLoadToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
