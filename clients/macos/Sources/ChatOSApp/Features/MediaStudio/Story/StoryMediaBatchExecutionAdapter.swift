import ChatOSCore
import Foundation

/// macOS execution adapter for persisted Story production queues.
///
/// Store records, provider models and job identities remain inside the adapter/session. The
/// Application layer only receives the safe batch projection plus the canonical project.
struct StoryMediaBatchExecutionAdapter: StoryMediaBatchExecuting {
    let store: StoryProjectStore
    let media: any MediaGenerationServicing
    let videoJobStore: MediaStudioVideoJobStore

    func prepareStoryMediaBatch(
        ownerUserID: String,
        project: StoryProject,
        kind: StoryMediaBatchKind,
        targetIDs: [String],
        availableModels: [MediaGenerationModel],
        userIdeas: String
    ) throws -> StoryMediaBatchPreparation {
        let batch = try StoryMediaBatch(
            project: project,
            owner: ownerUserID,
            kind: Self.appKind(kind),
            targets: targetIDs,
            models: availableModels,
            userIdeas: userIdeas
        )
        return StoryMediaBatchPreparation(
            batchID: batch.id,
            ownerUserID: ownerUserID,
            projectID: project.id,
            projectFingerprint: batch.expectedProjectDigest,
            kind: kind,
            requestedTargetIDs: targetIDs,
            steps: batch.historySnapshot.steps,
            executionMode: kind == .videos ? .directVideos : .persistentBatch
        )
    }

    func executeStoryMediaBatch(
        ownerUserID: String,
        accountSessionID: UUID,
        command: StoryMediaBatchCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        onEvent: @escaping @Sendable (StoryMediaBatchExecutionEvent) async -> Void
    ) async throws -> StoryMediaBatchExecutionOutcome {
        switch command {
        case let .start(project, preparation, availableModels, userIdeas):
            let batch = try StoryMediaBatch(
                id: preparation.batchID,
                project: project,
                owner: ownerUserID,
                kind: Self.appKind(preparation.kind),
                targets: preparation.requestedTargetIDs,
                models: availableModels,
                userIdeas: userIdeas
            )
            guard batch.historySnapshot.steps == preparation.steps,
                  try StoryAgentRun.matchesPersistedDigest(
                    batch.expectedProjectDigest,
                    project: project
                  ) else { throw StoryAgentError.projectChanged }
            let existing = try await store.inspectMediaBatches(
                owner: ownerUserID,
                projectID: project.id
            )
            guard existing.unreadable == 0, !existing.hasUnfinished else {
                throw StoryBatchError.activeBatch
            }
            return try await execute(
                batch,
                accountSessionID: accountSessionID,
                continuationCheck: continuationCheck,
                shouldPause: shouldPause,
                onEvent: onEvent
            )
        case let .resume(projectID, batchID):
            let batch = try await store.loadMediaBatch(
                owner: ownerUserID,
                projectID: projectID,
                batchID: batchID
            )
            guard !batch.finished else { throw StoryAgentError.invalidRun }
            return try await execute(
                batch,
                accountSessionID: accountSessionID,
                continuationCheck: continuationCheck,
                shouldPause: shouldPause,
                onEvent: onEvent
            )
        case let .abandon(project, batchID):
            var batch = try await store.loadMediaBatch(
                owner: ownerUserID,
                projectID: project.id,
                batchID: batchID
            )
            guard !batch.finished else { throw StoryAgentError.invalidRun }
            try await continuationCheck()
            batch.draft = project
            batch.expectedProjectDigest = try StoryAgentRun.digest(project)
            for step in batch.steps where step.kind == .videos {
                guard var job = batch.jobs[step.id],
                      !job.completed,
                      let localJobID = job.videoJobRecordID,
                      let index = batch.draft.segments.firstIndex(where: {
                          $0.id == step.targetID
                      }),
                      batch.draft.segments[index].video == nil,
                      batch.draft.segments[index].attempt == nil,
                      (batch.draft.segments[index].videoJobRecordID == localJobID
                        || batch.draft.segments[index].videoJobRecordID == nil) else {
                    continue
                }
                let workload = MediaStudioVideoJobStore.Workload.storyBatchVideo(
                    projectID: batch.draft.id,
                    batchID: batch.id,
                    segmentID: step.targetID,
                    attemptID: localJobID
                )
                let snapshot = try await videoJobStore.snapshot(
                    id: localJobID,
                    owner: ownerUserID
                )
                guard snapshot.workload == workload else {
                    throw MediaStudioVideoJobStoreError.jobConflict
                }
                _ = try await videoJobStore.activate(
                    id: localJobID,
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID,
                    workload: workload
                )
                try await continuationCheck()
                _ = try await videoJobStore.releaseAfterVerifiedRetry(
                    id: localJobID,
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID,
                    workload: workload
                )
                try await continuationCheck()
                job.videoJobRecordID = nil
                batch.jobs[step.id] = job
                batch.draft.segments[index].videoJobRecordID = nil
                batch.draft.segments[index].error = nil
                batch.events.append(.init(detail: "已核对并释放视频任务：\(step.targetID)"))
            }
            let intents = Set(batch.jobs.values.map(\.intentID))
            for var resource in batch.draft.resources
                where resource.imageGenerationAttemptID.map(intents.contains) == true {
                resource.media.generationAttemptID = nil
                try batch.draft.replaceResource(resource)
            }
            for index in batch.draft.segments.indices {
                if batch.draft.segments[index].imageGenerationAttemptID.map(intents.contains) == true {
                    batch.draft.segments[index].imageGenerationAttemptID = nil
                }
                if batch.draft.segments[index].lastFrameGenerationAttemptID.map(intents.contains) == true {
                    batch.draft.segments[index].lastFrameGenerationAttemptID = nil
                }
            }
            batch.status = .abandoned
            batch.updatedAt = Date()
            batch.events.append(.init(detail: "用户核对后结束本批，保留已有结果及视频任务"))
            try await store.commitMediaBatch(batch)
            try await continuationCheck()
            return .init(batch: batch.historySnapshot, project: batch.draft)
        }
    }

    private func execute(
        _ batch: StoryMediaBatch,
        accountSessionID: UUID,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        onEvent: @escaping @Sendable (StoryMediaBatchExecutionEvent) async -> Void
    ) async throws -> StoryMediaBatchExecutionOutcome {
        try await continuationCheck()
        let service: any MediaGenerationServicing
        if let bindable = media as? any SessionBoundMediaGenerationServicing {
            service = try await bindable.boundToCurrentSession()
        } else {
            service = media
        }
        try await continuationCheck()
        let session = StoryMediaBatchSession(
            batch: batch,
            store: store,
            media: service,
            videoJobStore: videoJobStore,
            accountSessionID: accountSessionID,
            check: continuationCheck,
            shouldPause: shouldPause,
            publish: { value in
                await onEvent(.init(batch: value.historySnapshot, project: value.draft))
            },
            publishVideoProgress: { value, stepID, progress in
                var snapshot = value.historySnapshot
                if let index = snapshot.steps.firstIndex(where: { $0.id == stepID }) {
                    snapshot.steps[index].progressStatus = progress.status
                }
                await onEvent(.init(batch: snapshot, project: value.draft))
            }
        )
        let result = try await session.run()
        try await continuationCheck()
        return .init(batch: result.historySnapshot, project: result.draft)
    }

    private static func appKind(_ kind: StoryMediaBatchKind) -> StoryMediaBatch.Kind {
        switch kind {
        case .assets: .assets
        case .frames: .frames
        case .lastFrames: .lastFrames
        case .videos: .videos
        case .pipeline: .pipeline
        }
    }
}
