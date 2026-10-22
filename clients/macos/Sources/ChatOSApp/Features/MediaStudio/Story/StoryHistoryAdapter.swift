import ChatOSCore
import Foundation

/// macOS persistence adapter for Story history and startup recovery.
///
/// The Store's filenames, cursors, Agent checkpoints, model catalog, generation intents,
/// and provider job identities never cross this boundary.
struct StoryHistoryAdapter: StoryHistoryLoading {
    let store: StoryProjectStore
    let videoJobStore: MediaStudioVideoJobStore

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
        accountSessionID: UUID,
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
        var migrationIssueCount = 0
        var migratedProject: StoryProject?
        let migration = StoryMediaBatchMigrationCoordinator(
            store: store,
            videoJobStore: videoJobStore
        )
        for index in batches.indices {
            try Task.checkCancellation()
            if let recovered = try await store.reconcileCompletedVideoBatch(batches[index]) {
                batches[index] = recovered
            }
            do {
                batches[index] = try await migration.migrate(
                    batches[index],
                    owner: ownerUserID,
                    accountSessionID: accountSessionID
                )
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                migrationIssueCount += 1
                // A multi-step legacy batch may have committed earlier handoffs before a
                // later step failed. Publish the latest valid file instead of a stale page
                // snapshot, while preserving the original record if it cannot be reloaded.
                if let latest = try? await store.loadMediaBatch(
                    owner: ownerUserID,
                    projectID: projectID,
                    batchID: batches[index].id
                ) {
                    batches[index] = latest
                }
            }
            if batches[index].draft != page.batches[index].draft {
                migratedProject = batches[index].draft
            }
        }
        return StoryMediaBatchHistoryPage(
            batches: batches.map(\.historySnapshot),
            unreadableCount: page.unreadable,
            migrationIssueCount: migrationIssueCount,
            nextCursor: Self.historyCursor(page.nextCursor),
            migratedProject: migratedProject
        )
    }

    private static func storeCursor(_ cursor: StoryHistoryCursor?) -> StoryProjectStore.PageCursor? {
        cursor.map { .init(modifiedAt: $0.modifiedAt, filename: $0.recordID) }
    }

    private static func historyCursor(_ cursor: StoryProjectStore.PageCursor?) -> StoryHistoryCursor? {
        cursor.map { .init(modifiedAt: $0.modifiedAt, recordID: $0.filename) }
    }
}

/// Migrates only the bounded history page selected by `StoryProjectStore`. It never invokes a
/// provider: legacy jobs with an identity become resume-only records, while jobs without one
/// become explicit manual-review records. Each step uses the same stage -> business handoff ->
/// activate protocol as live Batch execution, so every crash point is safe to replay.
private struct StoryMediaBatchMigrationCoordinator: Sendable {
    let store: StoryProjectStore
    let videoJobStore: MediaStudioVideoJobStore

    func migrate(
        _ input: StoryMediaBatch,
        owner: String,
        accountSessionID: UUID
    ) async throws -> StoryMediaBatch {
        guard !input.finished else { return input }
        var batch = input
        for step in batch.steps where step.kind == .videos {
            try Task.checkCancellation()
            guard let job = batch.jobs[step.id], !job.completed else { continue }
            let workload = MediaStudioVideoJobStore.Workload.storyBatchVideo(
                projectID: batch.draft.id,
                batchID: batch.id,
                segmentID: step.targetID,
                attemptID: job.intentID
            )

            if let recordID = job.videoJobRecordID {
                guard recordID == job.intentID else { throw StoryError.unresolvedSubmission }
                try await activateIfStaged(
                    id: recordID,
                    owner: owner,
                    accountSessionID: accountSessionID,
                    workload: workload
                )
                continue
            }

            guard let segment = batch.draft.segments.first(where: { $0.id == step.targetID }),
                  segment.video == nil,
                  segment.videoJobRecordID == nil,
                  let attempt = segment.attempt,
                  attempt.jobID == job.jobID else {
                throw StoryError.unresolvedSubmission
            }
            let request = VideoGenerationRequest(
                modelConfigID: attempt.modelConfigID,
                prompt: attempt.prompt,
                size: attempt.size,
                seconds: attempt.seconds,
                ratio: attempt.ratio
            )
            let existing = try await videoJobStore.snapshotIfPresent(
                id: job.intentID,
                owner: owner
            )
            if let existing, existing.status == .prepared {
                // A current-version Batch may have durably copied its create inputs and then
                // crashed before the business handoff. Preserve that safe create record; an
                // uncertain legacy import would discard its inputs and make recovery impossible.
                guard existing.workload == workload,
                      existing.providerJobID == nil,
                      job.jobID == nil else {
                    throw MediaStudioVideoJobStoreError.jobConflict
                }
            } else if let providerJobID = job.jobID {
                _ = try await videoJobStore.stageImportedProviderJob(
                    request: request,
                    id: job.intentID,
                    providerJobID: providerJobID,
                    owner: owner,
                    sessionEpoch: accountSessionID,
                    workload: workload
                )
            } else {
                _ = try await videoJobStore.stageSubmissionUncertain(
                    request: request,
                    id: job.intentID,
                    owner: owner,
                    sessionEpoch: accountSessionID,
                    workload: workload
                )
            }
            try Task.checkCancellation()
            batch = try await store.handoffMediaBatchVideoGenerationToMediaJob(
                owner: owner,
                projectID: batch.draft.id,
                batchID: batch.id,
                stepID: step.id,
                expectedIntentID: job.intentID
            )
            try Task.checkCancellation()
            try await activateIfStaged(
                id: job.intentID,
                owner: owner,
                accountSessionID: accountSessionID,
                workload: workload
            )
        }
        return batch
    }

    private func activateIfStaged(
        id: UUID,
        owner: String,
        accountSessionID: UUID,
        workload: MediaStudioVideoJobStore.Workload
    ) async throws {
        let snapshot = try await videoJobStore.snapshot(id: id, owner: owner)
        guard snapshot.workload == workload else {
            throw MediaStudioVideoJobStoreError.jobConflict
        }
        guard !snapshot.isActive else { return }
        let execution = try await videoJobStore.activate(
            id: id,
            owner: owner,
            sessionEpoch: accountSessionID,
            workload: workload
        )
        // Migration only publishes ownership. Releasing this value immediately releases the
        // execution lease; provider work starts solely from the explicit resume path.
        _ = execution.snapshot
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
