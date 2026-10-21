import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story history application boundary")
struct StoryHistoryUseCaseTests {
    @Test("initial history routes normalized identity and a bounded page size")
    func routesInitialHistory() async throws {
        let project = Self.project()
        let history = RecordingStoryHistory(
            runPage: .init(runs: []),
            batchPage: .init(batches: [])
        )
        let useCase = StoryHistoryUseCase(history: history, pageSize: 500)

        _ = try await useCase.loadInitial(
            ownerUserID: "  alice  ",
            projectID: project.id,
            currentProject: project
        )

        #expect(await history.requests == [
            .init(
                kind: .runs,
                ownerUserID: "alice",
                projectID: project.id,
                cursor: nil,
                limit: 200,
                includesCurrentProject: true
            ),
            .init(
                kind: .batches,
                ownerUserID: "alice",
                projectID: project.id,
                cursor: nil,
                limit: 200,
                includesCurrentProject: false
            ),
        ])
    }

    @Test("pages are newest first, deduplicated, and retain safe recovery state")
    func normalizesPages() async throws {
        let project = Self.project()
        let olderRun = Self.run(project: project, updatedAt: 10)
        let newerRun = Self.run(project: project, updatedAt: 30)
        var duplicateRun = olderRun
        duplicateRun.updatedAt = Date(timeIntervalSince1970: 5)
        let olderBatch = Self.batch(projectID: project.id, updatedAt: 20)
        let newerBatch = Self.batch(projectID: project.id, updatedAt: 40)
        var duplicateBatch = olderBatch
        duplicateBatch.updatedAt = Date(timeIntervalSince1970: 1)
        let history = RecordingStoryHistory(
            runPage: .init(
                runs: [olderRun, newerRun, duplicateRun],
                unreadableCount: -3,
                recoveredProject: project
            ),
            batchPage: .init(
                batches: [olderBatch, newerBatch, duplicateBatch],
                unreadableCount: 2
            )
        )

        let page = try await StoryHistoryUseCase(history: history).loadInitial(
            ownerUserID: "alice",
            projectID: project.id,
            currentProject: project
        )

        #expect(page.planningRuns.runs.map(\.id) == [newerRun.id, olderRun.id])
        #expect(page.planningRuns.unreadableCount == 0)
        #expect(page.planningRuns.recoveredProject == project)
        #expect(page.mediaBatches.batches.map(\.id) == [newerBatch.id, olderBatch.id])
        #expect(page.mediaBatches.unreadableCount == 2)
    }

    @Test("blank owners and foreign records fail closed")
    func rejectsInvalidIdentity() async {
        let project = Self.project()
        let history = RecordingStoryHistory(
            runPage: .init(runs: []),
            batchPage: .init(batches: [])
        )
        let useCase = StoryHistoryUseCase(history: history)

        await #expect(throws: StoryHistoryError.invalidOwner) {
            _ = try await useCase.loadInitial(
                ownerUserID: "   ",
                projectID: project.id,
                currentProject: project
            )
        }
        #expect(await history.requests.isEmpty)

        let foreignHistory = RecordingStoryHistory(
            runPage: .init(runs: [Self.run(project: project, owner: "bob", updatedAt: 1)]),
            batchPage: .init(batches: [])
        )
        await #expect(throws: StoryHistoryError.identityMismatch) {
            _ = try await StoryHistoryUseCase(history: foreignHistory).loadInitial(
                ownerUserID: "alice",
                projectID: project.id,
                currentProject: project
            )
        }
        #expect(await foreignHistory.requests.count == 1)
    }

    @Test("load lifecycle isolates lanes and rejects account, project, and session changes")
    func isolatesLoadLifecycle() {
        var state = StoryHistoryLoadState()
        let session = UUID()
        let projectID = UUID()
        let cursor = StoryHistoryCursor(modifiedAt: .now, recordID: "history.json")
        let runToken = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            kind: .planningRuns(projectID: projectID, cursor: cursor)
        )
        let batchToken = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            kind: .mediaBatches(projectID: projectID, cursor: cursor)
        )

        #expect(state.accepts(
            runToken,
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: projectID
        ))
        #expect(state.accepts(
            batchToken,
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: projectID
        ))
        #expect(!state.accepts(
            runToken,
            ownerUserID: "bob",
            accountSessionID: session,
            projectID: projectID
        ))
        #expect(!state.accepts(
            runToken,
            ownerUserID: "alice",
            accountSessionID: UUID(),
            projectID: projectID
        ))
        #expect(!state.accepts(
            runToken,
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: UUID()
        ))

        let initialToken = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            kind: .initial(projectID: projectID)
        )
        #expect(!state.accepts(
            runToken,
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: projectID
        ))
        #expect(!state.accepts(
            batchToken,
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: projectID
        ))
        let didFinish = state.finish(initialToken)
        #expect(didFinish)
        #expect(state.activeTokens.isEmpty)
    }

    private static func project() -> StoryProject {
        StoryProject(
            title: "Story",
            description: "",
            models: .init(
                textModelID: "text",
                imageModelID: "image",
                videoModelID: "video"
            )
        )
    }

    private static func run(
        project: StoryProject,
        owner: String = "alice",
        updatedAt: TimeInterval
    ) -> StoryPlanningRunSnapshot {
        StoryPlanningRunSnapshot(
            id: UUID(),
            ownerUserID: owner,
            projectID: project.id,
            stage: .outline,
            targetIDs: [],
            baseProjectFingerprint: "fingerprint",
            draft: project,
            events: [],
            status: .paused,
            stopReason: nil,
            modelCalls: 0,
            usesCloudMemory: true,
            applied: false,
            abandonedAt: nil,
            updatedAt: Date(timeIntervalSince1970: updatedAt)
        )
    }

    private static func batch(
        projectID: UUID,
        owner: String = "alice",
        updatedAt: TimeInterval
    ) -> StoryMediaBatchSnapshot {
        let step = StoryMediaBatchStepSnapshot(
            id: "assets:character-1",
            kind: .assets,
            targetID: "character-1",
            completed: true
        )
        return StoryMediaBatchSnapshot(
            id: UUID(),
            ownerUserID: owner,
            projectID: projectID,
            kind: .assets,
            status: .completed,
            steps: [step],
            events: [],
            consentAt: Date(timeIntervalSince1970: 0),
            error: nil,
            updatedAt: Date(timeIntervalSince1970: updatedAt)
        )
    }
}

private actor RecordingStoryHistory: StoryHistoryLoading {
    enum Kind: Equatable {
        case runs
        case batches
    }

    struct Request: Equatable {
        var kind: Kind
        var ownerUserID: String
        var projectID: UUID
        var cursor: StoryHistoryCursor?
        var limit: Int
        var includesCurrentProject: Bool
    }

    let runPage: StoryPlanningRunHistoryPage
    let batchPage: StoryMediaBatchHistoryPage
    var requests: [Request] = []

    init(
        runPage: StoryPlanningRunHistoryPage,
        batchPage: StoryMediaBatchHistoryPage
    ) {
        self.runPage = runPage
        self.batchPage = batchPage
    }

    func loadStoryPlanningRuns(
        ownerUserID: String,
        projectID: UUID,
        currentProject: StoryProject?,
        after cursor: StoryHistoryCursor?,
        limit: Int
    ) async throws -> StoryPlanningRunHistoryPage {
        requests.append(.init(
            kind: .runs,
            ownerUserID: ownerUserID,
            projectID: projectID,
            cursor: cursor,
            limit: limit,
            includesCurrentProject: currentProject != nil
        ))
        return runPage
    }

    func loadStoryMediaBatches(
        ownerUserID: String,
        projectID: UUID,
        after cursor: StoryHistoryCursor?,
        limit: Int
    ) async throws -> StoryMediaBatchHistoryPage {
        requests.append(.init(
            kind: .batches,
            ownerUserID: ownerUserID,
            projectID: projectID,
            cursor: cursor,
            limit: limit,
            includesCurrentProject: false
        ))
        return batchPage
    }
}
