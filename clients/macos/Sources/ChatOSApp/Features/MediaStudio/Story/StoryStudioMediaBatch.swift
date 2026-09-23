import AppKit
import ChatOSAgentRuntime
import ChatOSCore
import Foundation

@MainActor
extension StoryStudioViewModel {
    func previewMediaBatch(kind: StoryMediaBatch.Kind, targets: [String], models: [MediaGenerationModel],
                           userIdeas: String = "") throws -> StoryMediaBatch {
        guard let project, let owner else { throw StoryError.invalidProject }
        return try .init(project: project, owner: owner, kind: kind, targets: targets,
                         models: models, userIdeas: userIdeas)
    }
    func startMediaBatch(_ batch: StoryMediaBatch) {
        guard let project, batch.draft.id == project.id else { return }
        if batch.kind == .videos {
            selectedSegments = Set(batch.steps.map(\.targetID))
            generateBatch(availableModels: batch.models, userIdeas: batch.userIdeas ?? "")
            return
        }
        run("准备批量制作") { owner, token in
            guard owner == batch.owner,
                  try StoryAgentRun.matchesPersistedDigest(batch.expectedProjectDigest, project: project) else {
                throw StoryAgentError.projectChanged
            }
            let existing = try await self.store.loadMediaBatches(owner: owner, projectID: project.id)
            guard existing.unreadable == 0, !existing.batches.contains(where: { !$0.finished }) else { throw StoryBatchError.activeBatch }
            try await self.executeMediaBatch(batch, token: token)
        }
    }
    func resumeMediaBatch(_ id: UUID) {
        guard let project else { return }
        run("恢复原制作批次") { owner, token in
            let saved = try await self.store.loadMediaBatches(owner: owner, projectID: project.id)
            guard let batch = saved.batches.first(where: { $0.id == id }), !batch.finished else { throw StoryAgentError.invalidRun }
            try await self.executeMediaBatch(batch, token: token)
        }
    }
    /// Explicit UI confirmation only. Unresolved video intents are retained for the existing
    /// verified-job recovery UI; only this batch's image locks can be released.
    func abandonMediaBatch(_ id: UUID) {
        guard let project else { return }
        run("结束已核对的制作批次") { owner, token in
            let history = try await self.store.loadMediaBatches(owner: owner, projectID: project.id)
            guard var batch = history.batches.first(where: { $0.id == id }), !batch.finished else { throw StoryAgentError.invalidRun }
            try self.check(token)
            batch.draft = project; batch.expectedProjectDigest = try StoryAgentRun.digest(project)
            let intents = Set(batch.jobs.values.map(\.intentID))
            for var resource in batch.draft.resources where resource.imageGenerationAttemptID.map(intents.contains) == true {
                resource.media.generationAttemptID = nil
                try batch.draft.replaceResource(resource)
            }
            for i in batch.draft.segments.indices {
                if batch.draft.segments[i].imageGenerationAttemptID.map(intents.contains) == true {
                    batch.draft.segments[i].imageGenerationAttemptID = nil
                }
                if batch.draft.segments[i].lastFrameGenerationAttemptID.map(intents.contains) == true {
                    batch.draft.segments[i].lastFrameGenerationAttemptID = nil
                }
            }
            batch.status = .abandoned; batch.updatedAt = Date()
            batch.events.append(.init(detail: "用户核对后结束本批，保留已有结果及视频任务"))
            try await self.store.commitMediaBatch(batch)
            self.publishMediaBatch(batch, token: token)
        }
    }
    func executeMediaBatch(_ batch: StoryMediaBatch, token: UUID) async throws {
        try check(token)
        let service = try await boundMedia(token)
        let coordinator = StoryMediaBatchSession(batch: batch, store: store, media: service,
            check: { [weak self] in guard let self else { throw CancellationError() }; try await self.check(token) },
            shouldPause: { [weak self] in await self?.shouldPauseAgent(token) ?? true },
            publish: { [weak self] snapshot in await self?.publishMediaBatch(snapshot, token: token) })
        let result = try await coordinator.run()
        try check(token)
        if let error = result.error { errorMessage = error }
    }
    func boundMedia(_ token: UUID) async throws -> any MediaGenerationServicing {
        try check(token)
        let service: any MediaGenerationServicing
        if let bindable = media as? any SessionBoundMediaGenerationServicing {
            service = try await bindable.boundToCurrentSession()
        } else { service = media }
        try check(token)
        return service
    }
    func publishMediaBatch(_ batch: StoryMediaBatch, token: UUID, updateProject: Bool = true) {
        guard session == token, owner == batch.owner else { return }
        if let index = mediaBatches.firstIndex(where: { $0.id == batch.id }) { mediaBatches[index] = batch }
        else { mediaBatches.append(batch) }
        mediaBatches.sort { $0.updatedAt > $1.updatedAt }
        if updateProject, let index = projects.firstIndex(where: { $0.id == batch.draft.id }) { projects[index] = batch.draft }
        if isBusy, activeProjectID == batch.draft.id {
            operation = "\(batch.completedCount) / \(batch.steps.count) · \(batch.events.last?.detail ?? "准备制作")"
            if let step = batch.steps.first(where: { batch.jobs[$0.id] != nil && batch.jobs[$0.id]?.completed != true }), step.kind == .videos {
                activeSegmentID = step.targetID; selectedSegmentID = step.targetID
                progress = .init(status: batch.draft.segments.first { $0.id == step.targetID }?.attempt?.status ?? "submitting", jobID: batch.jobs[step.id]?.jobID)
            }
        }
    }
    func run(_ label: String, body: @escaping @MainActor (String, UUID) async throws -> Void) {
        guard !isBusy, !isLoading, activeAssetGenerations.isEmpty, activeFrameGenerations.isEmpty,
              let owner else {
            if !activeAssetGenerations.isEmpty || !activeFrameGenerations.isEmpty {
                errorMessage = Self.imageGenerationLockNotice
            }
            return
        }
        let token = session
        isBusy = true; operation = label; errorMessage = nil; pauseRequested = false; progress = nil; activeSegmentID = nil
        streamingModelText = ""; streamingToolName = nil
        activeProjectID = selectedProjectID
        task = Task {
            do { try await body(owner, token) }
            catch { if session == token { errorMessage = error.localizedDescription } }
            guard session == token else { return }
            isBusy = false; operation = ""; task = nil; progress = nil; activeSegmentID = nil; activeProjectID = nil
        }
    }
    func clearImageGenerationLockNoticeIfNeeded() {
        guard activeAssetGenerations.isEmpty, activeFrameGenerations.isEmpty,
              errorMessage == Self.imageGenerationLockNotice else { return }
        errorMessage = nil
    }
    func check(_ token: UUID) throws {
        try Task.checkCancellation()
        guard session == token else { throw CancellationError() }
    }
    func commit(_ project: StoryProject, owner: String, token: UUID) async throws {
        try check(token)
        var next = project
        StoryContinuityContext.reconcileInheritedFirstFrames(&next)
        next.updatedAt = Date()
        let activeSegmentIDs = Set(activeVideoGenerations.lazy
            .filter { $0.projectID == next.id }
            .map(\.segmentID))
        next = try await store.save(
            next, owner: owner, preservingVideoSegmentIDs: activeSegmentIDs
        )
        try check(token)
        if let index = projects.firstIndex(where: { $0.id == next.id }) { projects[index] = next }
        else { projects.append(next) }
        projects.sort { $0.updatedAt > $1.updatedAt }
    }
    func publishProject(_ project: StoryProject, token: UUID) {
        guard session == token else { return }
        if let index = projects.firstIndex(where: { $0.id == project.id }) { projects[index] = project }
        else { projects.append(project) }
        projects.sort { $0.updatedAt > $1.updatedAt }
    }
}
