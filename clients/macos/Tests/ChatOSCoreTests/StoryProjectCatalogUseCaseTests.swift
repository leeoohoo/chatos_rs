import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story project catalog application boundary")
struct StoryProjectCatalogUseCaseTests {
    @Test("initial and next pages route a normalized owner and bounded page size")
    func routesPageRequests() async throws {
        let cursor = StoryProjectCatalogCursor(
            modifiedAt: Date(timeIntervalSince1970: 20),
            recordID: "project-2"
        )
        let catalog = RecordingStoryProjectCatalog()
        let useCase = StoryProjectCatalogUseCase(catalog: catalog, pageSize: 500)

        _ = try await useCase.loadInitial(ownerUserID: "  alice  ")
        _ = try await useCase.loadNext(ownerUserID: "alice", after: cursor)

        #expect(await catalog.requests == [
            .init(ownerUserID: "alice", cursor: nil, limit: 200),
            .init(ownerUserID: "alice", cursor: cursor, limit: 200),
        ])
    }

    @Test("next page merge deduplicates, sorts, and accumulates unreadable projects")
    func mergesNextPage() {
        let catalog = RecordingStoryProjectCatalog()
        let useCase = StoryProjectCatalogUseCase(catalog: catalog)
        let duplicate = Self.project(title: "duplicate", updatedAt: 20)
        let newest = Self.project(title: "newest", updatedAt: 30)
        let oldest = Self.project(title: "oldest", updatedAt: 10)
        let cursor = StoryProjectCatalogCursor(
            modifiedAt: Date(timeIntervalSince1970: 10),
            recordID: oldest.id.uuidString
        )
        let current = StoryProjectCatalogCollection(
            projects: [duplicate, newest],
            unreadableCount: 1
        )
        var staleDuplicate = duplicate
        staleDuplicate.updatedAt = Date(timeIntervalSince1970: 5)

        let merged = useCase.merging(
            .init(
                projects: [oldest, staleDuplicate],
                unreadableCount: 2,
                nextCursor: cursor
            ),
            into: current
        )

        #expect(merged.projects.map(\.title) == ["newest", "duplicate", "oldest"])
        #expect(merged.unreadableCount == 3)
        #expect(merged.nextCursor == cursor)
    }

    @Test("blank owners fail before the catalog and lifecycle rejects replaced requests")
    func validatesOwnerAndLifecycle() async {
        let catalog = RecordingStoryProjectCatalog()
        let useCase = StoryProjectCatalogUseCase(catalog: catalog)
        await #expect(throws: StoryProjectCatalogError.invalidOwner) {
            _ = try await useCase.loadInitial(ownerUserID: "   ")
        }
        #expect(await catalog.requests.isEmpty)

        var state = StoryProjectCatalogLoadState()
        let session = UUID()
        let cursor = StoryProjectCatalogCursor(modifiedAt: .now, recordID: "project")
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

    private static func project(title: String, updatedAt: TimeInterval) -> StoryProject {
        var project = StoryProject(
            title: title,
            description: "",
            models: .init(
                textModelID: "text",
                imageModelID: "image",
                videoModelID: "video"
            )
        )
        project.updatedAt = Date(timeIntervalSince1970: updatedAt)
        return project
    }
}

private actor RecordingStoryProjectCatalog: StoryProjectCatalogLoading {
    struct Request: Equatable {
        var ownerUserID: String
        var cursor: StoryProjectCatalogCursor?
        var limit: Int
    }

    var requests: [Request] = []

    func loadStoryProjects(
        ownerUserID: String,
        after cursor: StoryProjectCatalogCursor?,
        limit: Int
    ) async throws -> StoryProjectCatalogPage {
        requests.append(.init(ownerUserID: ownerUserID, cursor: cursor, limit: limit))
        return .init(projects: [])
    }
}
