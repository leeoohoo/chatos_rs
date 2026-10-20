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
        let store = store
        let media = media
        run("准备批量制作") { [weak self, store, media] owner, token in
            guard owner == batch.owner,
                  try StoryAgentRun.matchesPersistedDigest(batch.expectedProjectDigest, project: project) else {
                throw StoryAgentError.projectChanged
            }
            let existing = try await store.inspectMediaBatches(owner: owner, projectID: project.id)
            guard existing.unreadable == 0, !existing.hasUnfinished else { throw StoryBatchError.activeBatch }
            let result = try await Self.executeMediaBatch(
                batch,
                store: store,
                media: media,
                check: { [weak self] in
                    guard let self else { throw CancellationError() }
                    try await self.check(token)
                },
                shouldPause: { [weak self] in await self?.shouldPauseAgent(token) ?? true },
                publish: { [weak self] snapshot in
                    await self?.publishMediaBatch(snapshot, token: token)
                }
            )
            if let error = result.error { self?.errorMessage = error }
        }
    }
    func resumeMediaBatch(_ id: UUID) {
        guard let project else { return }
        let store = store
        let media = media
        run("恢复原制作批次") { [weak self, store, media] owner, token in
            let batch = try await store.loadMediaBatch(
                owner: owner,
                projectID: project.id,
                batchID: id
            )
            guard !batch.finished else { throw StoryAgentError.invalidRun }
            let result = try await Self.executeMediaBatch(
                batch,
                store: store,
                media: media,
                check: { [weak self] in
                    guard let self else { throw CancellationError() }
                    try await self.check(token)
                },
                shouldPause: { [weak self] in await self?.shouldPauseAgent(token) ?? true },
                publish: { [weak self] snapshot in
                    await self?.publishMediaBatch(snapshot, token: token)
                }
            )
            if let error = result.error { self?.errorMessage = error }
        }
    }
    /// Explicit UI confirmation only. Unresolved video intents are retained for the existing
    /// verified-job recovery UI; only this batch's image locks can be released.
    func abandonMediaBatch(_ id: UUID) {
        guard let project else { return }
        let store = store
        run("结束已核对的制作批次") { [weak self, store] owner, token in
            var batch = try await store.loadMediaBatch(
                owner: owner,
                projectID: project.id,
                batchID: id
            )
            guard !batch.finished else { throw StoryAgentError.invalidRun }
            try Task.checkCancellation()
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
            try await store.commitMediaBatch(batch)
            try Task.checkCancellation()
            self?.publishMediaBatch(batch, token: token)
        }
    }
    nonisolated static func executeMediaBatch(
        _ batch: StoryMediaBatch,
        store: StoryProjectStore,
        media: any MediaGenerationServicing,
        check: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        publish: @escaping @Sendable (StoryMediaBatch) async -> Void
    ) async throws -> StoryMediaBatch {
        try await check()
        let service: any MediaGenerationServicing
        if let bindable = media as? any SessionBoundMediaGenerationServicing {
            service = try await bindable.boundToCurrentSession()
        } else {
            service = media
        }
        try await check()
        let coordinator = StoryMediaBatchSession(
            batch: batch,
            store: store,
            media: service,
            check: check,
            shouldPause: shouldPause,
            publish: publish
        )
        let result = try await coordinator.run()
        try await check()
        return result
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
        task = Task { [weak self] in
            let result: Result<Void, Error>
            do {
                try await body(owner, token)
                result = .success(())
            } catch {
                result = .failure(error)
            }
            guard let self, self.session == token else { return }
            if case .failure(let error) = result, !(error is CancellationError) {
                self.errorMessage = error.localizedDescription
            }
            self.isBusy = false
            self.operation = ""
            self.task = nil
            self.progress = nil
            self.activeSegmentID = nil
            self.activeProjectID = nil
        }
    }
    func runCommit(
        _ label: String,
        project: StoryProject,
        completion: @escaping @MainActor (StoryStudioViewModel, StoryProject) -> Void = { _, _ in }
    ) {
        let store = store
        let preservingVideoSegmentIDs = Set(activeVideoGenerations.lazy
            .filter { $0.projectID == project.id }
            .map(\.segmentID))
        run(label) { [weak self, store] owner, token in
            let saved = try await Self.saveStoryProject(
                project,
                owner: owner,
                store: store,
                preservingVideoSegmentIDs: preservingVideoSegmentIDs
            )
            guard let self, self.session == token else { return }
            self.publishProject(saved, token: token)
            completion(self, saved)
        }
    }
    nonisolated static func saveStoryProject(
        _ project: StoryProject,
        owner: String,
        store: StoryProjectStore,
        preservingVideoSegmentIDs: Set<String>
    ) async throws -> StoryProject {
        try Task.checkCancellation()
        var next = project
        StoryContinuityContext.reconcileInheritedFirstFrames(&next)
        next.updatedAt = Date()
        let saved = try await store.save(
            next,
            owner: owner,
            preservingVideoSegmentIDs: preservingVideoSegmentIDs
        )
        try Task.checkCancellation()
        return saved
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
