import Foundation
import Testing
@testable import ChatOSCore

@Suite("Media Studio history application boundary")
struct MediaStudioHistoryUseCaseTests {
    @Test("initial and next pages route owner cursor and bounded page size")
    func routesPageRequests() async throws {
        let cursor = MediaStudioHistoryCursor(
            modifiedAt: Date(timeIntervalSince1970: 20),
            recordID: "record-2"
        )
        let loader = RecordingHistoryLoader()
        let useCase = MediaStudioHistoryUseCase(history: loader, pageSize: 500)

        _ = try await useCase.loadInitial(owner: "  alice  ")
        _ = try await useCase.loadNext(owner: "alice", after: cursor)

        #expect(await loader.requests == [
            .init(owner: "alice", cursor: nil, limit: 200),
            .init(owner: "alice", cursor: cursor, limit: 200),
        ])
    }

    @Test("next page merge deduplicates sorts and accumulates unreadable records")
    func mergesNextPage() {
        let useCase = MediaStudioHistoryUseCase(history: RecordingHistoryLoader())
        let current = MediaStudioHistoryCollection(
            images: [Self.image(id: "new", createdAt: 30), Self.image(id: "duplicate", createdAt: 20)],
            videos: [Self.video(id: "video-new", createdAt: 40)],
            unreadableCount: 1,
            nextCursor: .init(modifiedAt: Date(timeIntervalSince1970: 20), recordID: "page-1")
        )
        let nextCursor = MediaStudioHistoryCursor(
            modifiedAt: Date(timeIntervalSince1970: 10),
            recordID: "page-2"
        )
        let page = MediaStudioHistoryPage(
            images: [Self.image(id: "old", createdAt: 10), Self.image(id: "duplicate", createdAt: 5)],
            videos: [Self.video(id: "video-old", createdAt: 15)],
            unreadableCount: 2,
            nextCursor: nextCursor
        )

        let merged = useCase.merging(page, into: current)

        #expect(merged.images.map(\.id) == ["new", "duplicate", "old"])
        #expect(merged.videos.map(\.id) == ["video-new", "video-old"])
        #expect(merged.unreadableCount == 3)
        #expect(merged.nextCursor == nextCursor)
    }

    @Test("lifecycle state rejects replaced owner session and page requests")
    func lifecycleRejectsStaleTokens() {
        var state = MediaStudioHistoryLoadState()
        let session = UUID()
        let cursor = MediaStudioHistoryCursor(
            modifiedAt: Date(timeIntervalSince1970: 10),
            recordID: "page"
        )
        let old = state.begin(ownerUserID: "alice", accountSessionID: session, kind: .initial)
        let current = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            kind: .next(cursor)
        )

        #expect(!state.accepts(old, ownerUserID: "alice", accountSessionID: session))
        #expect(!state.accepts(current, ownerUserID: "bob", accountSessionID: session))
        #expect(!state.accepts(current, ownerUserID: "alice", accountSessionID: UUID()))
        #expect(state.accepts(current, ownerUserID: "alice", accountSessionID: session))
        let didFinish = state.finish(current)
        #expect(didFinish)
        #expect(state.activeToken == nil)
    }

    private static func image(id: String, createdAt: TimeInterval) -> MediaStudioImageHistoryItem {
        .init(
            id: id,
            prompt: id,
            modelName: "image",
            createdAt: Date(timeIntervalSince1970: createdAt),
            images: []
        )
    }

    private static func video(id: String, createdAt: TimeInterval) -> MediaStudioVideoHistoryItem {
        .init(
            id: id,
            prompt: id,
            modelName: "video",
            createdAt: Date(timeIntervalSince1970: createdAt),
            fileURL: URL(fileURLWithPath: "/tmp/\(id).mp4")
        )
    }
}

private actor RecordingHistoryLoader: MediaStudioHistoryLoading {
    struct Request: Equatable {
        var owner: String
        var cursor: MediaStudioHistoryCursor?
        var limit: Int
    }

    var requests: [Request] = []

    func load(
        owner: String,
        after cursor: MediaStudioHistoryCursor?,
        limit: Int
    ) async throws -> MediaStudioHistoryPage {
        requests.append(.init(owner: owner, cursor: cursor, limit: limit))
        return .init()
    }
}
