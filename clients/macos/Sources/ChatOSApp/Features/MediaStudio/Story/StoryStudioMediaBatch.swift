import ChatOSCore
import Foundation

@MainActor
extension StoryStudioViewModel {
    func mediaBatchCandidates(kind: StoryMediaBatchKind) -> [String] {
        guard let project else { return [] }
        return storyMediaBatch.eligibleTargetIDs(project: project, kind: kind)
    }
    func previewMediaBatch(
        kind: StoryMediaBatchKind,
        targets: [String],
        models: [MediaGenerationModel],
        userIdeas: String = ""
    ) throws -> StoryMediaBatchPreparation {
        guard let project, let owner else { throw StoryError.invalidProject }
        return try storyMediaBatch.prepare(
            ownerUserID: owner,
            project: project,
            kind: kind,
            targetIDs: targets,
            availableModels: models,
            userIdeas: userIdeas
        )
    }
    func startMediaBatch(
        _ preparation: StoryMediaBatchPreparation,
        models: [MediaGenerationModel],
        userIdeas: String = ""
    ) {
        guard let project, preparation.projectID == project.id else { return }
        if preparation.executionMode == .directVideos {
            selectedSegments = Set(preparation.requestedTargetIDs)
            startDirectVideoQueue(
                preparation: preparation,
                availableModels: models,
                userIdeas: userIdeas
            )
            return
        }
        let storyMediaBatch = storyMediaBatch
        run("准备批量制作") { [weak self, storyMediaBatch] owner, token in
            guard let self,
                  let request = self.storyMediaBatchExecutionState.begin(
                    ownerUserID: owner,
                    accountSessionID: token,
                    kind: .start(
                        projectID: project.id,
                        batchID: preparation.batchID
                    )
                  ) else { throw CancellationError() }
            defer { _ = self.storyMediaBatchExecutionState.finish(request) }
            let result = try await storyMediaBatch.execute(
                ownerUserID: owner,
                command: .start(
                    project: project,
                    preparation: preparation,
                    availableModels: models,
                    userIdeas: userIdeas
                ),
                continuationCheck: { [weak self] in
                    guard let self else { throw CancellationError() }
                    try await self.checkMediaBatch(request, session: token)
                },
                shouldPause: { [weak self] in await self?.shouldPauseAgent(token) ?? true },
                onEvent: { [weak self] event in
                    await self?.receiveMediaBatchEvent(event, request: request, token: token)
                }
            )
            self.applyMediaBatchOutcome(result, request: request, token: token)
        }
    }
    func resumeMediaBatch(_ id: UUID) {
        guard let project else { return }
        let storyMediaBatch = storyMediaBatch
        run("恢复原制作批次") { [weak self, storyMediaBatch] owner, token in
            guard let self,
                  let request = self.storyMediaBatchExecutionState.begin(
                    ownerUserID: owner,
                    accountSessionID: token,
                    kind: .resume(projectID: project.id, batchID: id)
                  ) else { throw CancellationError() }
            defer { _ = self.storyMediaBatchExecutionState.finish(request) }
            let result = try await storyMediaBatch.execute(
                ownerUserID: owner,
                command: .resume(projectID: project.id, batchID: id),
                continuationCheck: { [weak self] in
                    guard let self else { throw CancellationError() }
                    try await self.checkMediaBatch(request, session: token)
                },
                shouldPause: { [weak self] in await self?.shouldPauseAgent(token) ?? true },
                onEvent: { [weak self] event in
                    await self?.receiveMediaBatchEvent(event, request: request, token: token)
                }
            )
            self.applyMediaBatchOutcome(result, request: request, token: token)
        }
    }
    /// Explicit UI confirmation only. Unresolved video intents are retained for the existing
    /// verified-job recovery UI; only this batch's image locks can be released.
    func abandonMediaBatch(_ id: UUID) {
        guard let project else { return }
        let storyMediaBatch = storyMediaBatch
        run("结束已核对的制作批次") { [weak self, storyMediaBatch] owner, token in
            guard let self,
                  let request = self.storyMediaBatchExecutionState.begin(
                    ownerUserID: owner,
                    accountSessionID: token,
                    kind: .abandon(projectID: project.id, batchID: id)
                  ) else { throw CancellationError() }
            defer { _ = self.storyMediaBatchExecutionState.finish(request) }
            let result = try await storyMediaBatch.execute(
                ownerUserID: owner,
                command: .abandon(project: project, batchID: id),
                continuationCheck: { [weak self] in
                    guard let self else { throw CancellationError() }
                    try await self.checkMediaBatch(request, session: token)
                },
                shouldPause: { false },
                onEvent: { [weak self] event in
                    await self?.receiveMediaBatchEvent(event, request: request, token: token)
                }
            )
            self.applyMediaBatchOutcome(result, request: request, token: token)
        }
    }
    private func checkMediaBatch(
        _ request: StoryMediaBatchExecutionToken,
        session token: UUID
    ) throws {
        try check(token)
        guard storyMediaBatchExecutionState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: token,
            projectID: activeProjectID
        ) else { throw CancellationError() }
    }
    private func receiveMediaBatchEvent(
        _ event: StoryMediaBatchExecutionEvent,
        request: StoryMediaBatchExecutionToken,
        token: UUID
    ) {
        guard storyMediaBatchExecutionState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: token,
            projectID: activeProjectID
        ) else { return }
        publishProject(event.project, token: token)
        publishMediaBatch(event.batch, token: token)
        if isBusy, activeProjectID == event.project.id {
            operation = "\(event.batch.completedCount) / \(event.batch.steps.count) · "
                + (event.batch.events.last?.detail ?? "准备制作")
            if let step = event.batch.steps.first(where: {
                !$0.completed && $0.kind == .videos && $0.progressStatus != nil
            }) {
                activeSegmentID = step.targetID
                selectedSegmentID = step.targetID
                progress = .init(status: step.progressStatus ?? "submitting")
            }
        }
    }
    private func applyMediaBatchOutcome(
        _ outcome: StoryMediaBatchExecutionOutcome,
        request: StoryMediaBatchExecutionToken,
        token: UUID
    ) {
        guard storyMediaBatchExecutionState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: token,
            projectID: activeProjectID
        ) else { return }
        publishProject(outcome.project, token: token)
        publishMediaBatch(outcome.batch, token: token)
        if let error = outcome.batch.error { errorMessage = error }
    }
    func publishMediaBatch(_ batch: StoryMediaBatchSnapshot, token: UUID) {
        guard session == token, owner == batch.ownerUserID else { return }
        if let index = mediaBatches.firstIndex(where: { $0.id == batch.id }) {
            if mediaBatches[index].updatedAt <= batch.updatedAt {
                mediaBatches[index] = batch
            }
        } else {
            mediaBatches.append(batch)
        }
        mediaBatches.sort {
            $0.updatedAt != $1.updatedAt
                ? $0.updatedAt > $1.updatedAt
                : $0.id.uuidString > $1.id.uuidString
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
        planningActivity.reset()
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
        guard let base = projects.first(where: { $0.id == project.id }) else {
            errorMessage = StoryProjectMutationError.projectMismatch.localizedDescription
            return
        }
        let preservingVideoSegmentIDs = Set(activeVideoGenerations.lazy
            .filter { $0.projectID == project.id }
            .map(\.segmentID))
        let command: StoryProjectEditCommand
        do {
            command = try .init(
                base: base,
                project: project,
                preservingVideoSegmentIDs: preservingVideoSegmentIDs
            )
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        let projectMutation = projectMutation
        run(label) { [weak self, projectMutation] owner, token in
            guard let self,
                  let request = self.projectMutationState.begin(
                    ownerUserID: owner,
                    accountSessionID: token,
                    kind: .edit(
                        projectID: command.projectID,
                        baseFingerprint: command.baseFingerprint
                    )
                  ) else { throw CancellationError() }
            defer { _ = self.projectMutationState.finish(request) }
            let saved = try await projectMutation.persistEdit(
                command,
                ownerUserID: owner
            )
            guard self.projectMutationState.accepts(
                request,
                ownerUserID: self.owner,
                accountSessionID: self.session
            ) else { return }
            self.publishProject(saved, token: token)
            completion(self, saved)
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
    func publishProject(_ project: StoryProject, token: UUID) {
        guard session == token else { return }
        if let index = projects.firstIndex(where: { $0.id == project.id }) { projects[index] = project }
        else { projects.append(project) }
        projects.sort { $0.updatedAt > $1.updatedAt }
    }
}
