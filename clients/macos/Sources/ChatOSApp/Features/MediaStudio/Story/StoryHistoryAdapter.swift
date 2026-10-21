import ChatOSCore
import Foundation

/// macOS persistence adapter for Story history and startup recovery.
///
/// The Store's filenames, cursors, Agent checkpoints, model catalog, generation intents,
/// and provider job identities never cross this boundary.
struct StoryHistoryAdapter: StoryHistoryLoading {
    let store: StoryProjectStore

    func loadStoryPlanningRuns(
        ownerUserID: String,
        projectID: UUID,
        currentProject: StoryProject?,
        after cursor: StoryHistoryCursor?,
        limit: Int
    ) async throws -> StoryPlanningRunHistoryPage {
        let page = try await store.loadRuns(
            owner: ownerUserID,
            projectID: projectID,
            after: Self.storeCursor(cursor),
            limit: limit
        )
        try Task.checkCancellation()
        var runs = page.runs
        var recoveredProject: StoryProject?

        // Recovery is an initial-page side effect only. Loading older history must remain
        // read-only and cannot unexpectedly replace the current project.
        if cursor == nil, let currentProject {
            let canonicalDigest = try StoryAgentRun.digest(currentProject)
            if let candidateIndex = runs.firstIndex(where: {
                !$0.applied && $0.abandonedAt == nil
                    && ((try? StoryAgentRun.matchesPersistedDigest(
                        $0.baseDigest,
                        project: currentProject
                    )) == true || (try? StoryAgentRun.digest($0.draft)) == canonicalDigest)
            }), let recovered = try await store.applyValidatedRunIfPossible(
                runs[candidateIndex],
                owner: ownerUserID
            ) {
                runs[candidateIndex] = recovered.0
                recoveredProject = recovered.1
            }
        }
        try Task.checkCancellation()
        return StoryPlanningRunHistoryPage(
            runs: runs.map(\.planningSnapshot),
            unreadableCount: page.unreadable,
            nextCursor: Self.historyCursor(page.nextCursor),
            recoveredProject: recoveredProject
        )
    }

    func loadStoryMediaBatches(
        ownerUserID: String,
        projectID: UUID,
        after cursor: StoryHistoryCursor?,
        limit: Int
    ) async throws -> StoryMediaBatchHistoryPage {
        let page = try await store.loadMediaBatches(
            owner: ownerUserID,
            projectID: projectID,
            after: Self.storeCursor(cursor),
            limit: limit
        )
        var batches = page.batches
        for index in batches.indices {
            try Task.checkCancellation()
            if let recovered = try await store.reconcileCompletedVideoBatch(batches[index]) {
                batches[index] = recovered
            }
        }
        return StoryMediaBatchHistoryPage(
            batches: batches.map(\.historySnapshot),
            unreadableCount: page.unreadable,
            nextCursor: Self.historyCursor(page.nextCursor)
        )
    }

    private static func storeCursor(_ cursor: StoryHistoryCursor?) -> StoryProjectStore.PageCursor? {
        cursor.map { .init(modifiedAt: $0.modifiedAt, filename: $0.recordID) }
    }

    private static func historyCursor(_ cursor: StoryProjectStore.PageCursor?) -> StoryHistoryCursor? {
        cursor.map { .init(modifiedAt: $0.modifiedAt, recordID: $0.filename) }
    }
}

extension StoryMediaBatch {
    var historySnapshot: StoryMediaBatchSnapshot {
        let snapshotKind: StoryMediaBatchKind = switch kind {
        case .assets: .assets
        case .frames: .frames
        case .lastFrames: .lastFrames
        case .videos: .videos
        case .pipeline: .pipeline
        }
        let snapshotStatus: StoryMediaBatchStatus = switch status {
        case .ready: .ready
        case .running: .running
        case .paused: .paused
        case .needsReview: .needsReview
        case .completed: .completed
        case .abandoned: .abandoned
        }
        return StoryMediaBatchSnapshot(
            id: id,
            ownerUserID: owner,
            projectID: draft.id,
            kind: snapshotKind,
            status: snapshotStatus,
            steps: steps.map { step in
                let snapshotKind: StoryMediaBatchKind = switch step.kind {
                case .assets: .assets
                case .frames: .frames
                case .lastFrames: .lastFrames
                case .videos: .videos
                case .pipeline: .pipeline
                }
                return StoryMediaBatchStepSnapshot(
                    id: step.id,
                    kind: snapshotKind,
                    targetID: step.targetID,
                    completed: jobs[step.id]?.completed == true,
                    progressStatus: step.kind == .videos
                        ? draft.segments.first(where: { $0.id == step.targetID })?.attempt?.status
                        : nil
                )
            },
            events: events.map {
                StoryMediaBatchEventSnapshot(id: $0.id, date: $0.date, detail: $0.detail)
            },
            consentAt: consentAt,
            error: error,
            updatedAt: updatedAt
        )
    }
}
