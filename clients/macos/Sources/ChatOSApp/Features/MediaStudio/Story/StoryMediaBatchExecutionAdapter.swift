import ChatOSCore
import Foundation

/// macOS execution adapter for persisted Story production queues.
///
/// Store records, provider models and job identities remain inside the adapter/session. The
/// Application layer only receives the safe batch projection plus the canonical project.
struct StoryMediaBatchExecutionAdapter: StoryMediaBatchExecuting {
    let store: StoryProjectStore
    let media: any MediaGenerationServicing

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
            check: continuationCheck,
            shouldPause: shouldPause,
            publish: { value in
                await onEvent(.init(batch: value.historySnapshot, project: value.draft))
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
