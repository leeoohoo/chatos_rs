import Foundation
import Testing
@testable import ChatOSCore

@Suite("ProjectRemovalUseCase")
struct ProjectRemovalUseCaseTests {
    @Test
    func freezesOwnerProjectAndRevisionForRemoval() async throws {
        let service = ProjectRemovalUseCaseService(record: Self.record(revision: 7))
        let request = ProjectRemovalRequest(
            ownerUserID: "owner",
            accountGeneration: 12,
            projectID: "project"
        )

        let receipt = try await ProjectRemovalUseCase(service: service).remove(request)

        #expect(receipt == .init(request: request, removedRevision: 7))
        let removals = await service.removalSnapshot()
        let expected: [ProjectRemovalUseCaseService.Removal] = [
            .init(ownerUserID: "owner", projectID: "project", revision: 7),
        ]
        #expect(removals == expected)
    }

    @Test
    func missingProjectFailsWithoutMutation() async {
        let service = ProjectRemovalUseCaseService(record: nil)
        let request = ProjectRemovalRequest(
            ownerUserID: "owner",
            accountGeneration: 1,
            projectID: "missing"
        )

        await #expect(throws: ProjectRegistryError.notFound) {
            try await ProjectRemovalUseCase(service: service).remove(request)
        }
        #expect(await service.removalCount() == 0)
    }

    @Test
    func cancellationAfterLookupPreventsRemoval() async {
        let service = ProjectRemovalUseCaseService(
            record: Self.record(revision: 3),
            suspendLookup: true
        )
        let request = ProjectRemovalRequest(
            ownerUserID: "owner",
            accountGeneration: 2,
            projectID: "project"
        )
        let task = Task {
            try await ProjectRemovalUseCase(service: service).remove(request)
        }
        await service.waitForLookup()

        task.cancel()
        await service.resumeLookup()

        await #expect(throws: CancellationError.self) { try await task.value }
        #expect(await service.removalCount() == 0)
    }

    private static func record(revision: Int64) -> LocalProjectRecord {
        .init(
            id: "project",
            ownerUserID: "owner",
            draft: .init(name: "Project", workspaceID: "workspace"),
            revision: revision,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
    }
}

private actor ProjectRemovalUseCaseService: ProjectRemovalServicing {
    struct Removal: Sendable, Equatable {
        let ownerUserID: String
        let projectID: String
        let revision: Int64
    }

    private let record: LocalProjectRecord?
    private let suspendLookup: Bool
    private var lookupStarted = false
    private var lookupContinuation: CheckedContinuation<Void, Never>?
    private(set) var removals: [Removal] = []

    init(record: LocalProjectRecord?, suspendLookup: Bool = false) {
        self.record = record
        self.suspendLookup = suspendLookup
    }

    func projectRecord(ownerUserID: String, projectID: String) async throws -> LocalProjectRecord? {
        if suspendLookup {
            lookupStarted = true
            await withCheckedContinuation { lookupContinuation = $0 }
        }
        return record
    }

    func removeProject(
        ownerUserID: String,
        projectID: String,
        expectedRevision: Int64
    ) async throws {
        removals.append(.init(
            ownerUserID: ownerUserID,
            projectID: projectID,
            revision: expectedRevision
        ))
    }

    func waitForLookup() async {
        while !lookupStarted { await Task.yield() }
    }

    func resumeLookup() {
        lookupContinuation?.resume()
        lookupContinuation = nil
    }

    func removalCount() -> Int { removals.count }
    func removalSnapshot() -> [Removal] { removals }
}
