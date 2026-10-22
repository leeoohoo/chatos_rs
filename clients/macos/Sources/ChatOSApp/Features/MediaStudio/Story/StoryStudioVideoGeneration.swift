import AppKit
import ChatOSAgentRuntime
import ChatOSCore
import Foundation

@MainActor
extension StoryStudioViewModel {
    func generateBatch(availableModels: [MediaGenerationModel], userIdeas: String = "") {
        guard !isBusy, !isLoading, videoBatchTask == nil, let owner, let project else { return }
        let ids = project.segments.filter { selectedSegments.contains($0.id) && $0.isReady }.map(\.id)
        guard !ids.isEmpty else { return }
        do {
            let preparation = try storyMediaBatch.prepare(
                ownerUserID: owner,
                project: project,
                kind: .videos,
                targetIDs: ids,
                availableModels: availableModels,
                userIdeas: userIdeas
            )
            startDirectVideoQueue(
                preparation: preparation,
                availableModels: availableModels,
                userIdeas: userIdeas
            )
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func startDirectVideoQueue(
        preparation: StoryMediaBatchPreparation,
        availableModels: [MediaGenerationModel],
        userIdeas: String
    ) {
        guard !isBusy, !isLoading, videoBatchTask == nil,
              let owner, let project else { return }
        let plan: StoryDirectVideoQueuePlan
        do {
            plan = try storyDirectVideoQueue.prepare(
                ownerUserID: owner,
                project: project,
                preparation: preparation,
                availableModels: availableModels,
                userIdeas: userIdeas
            )
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        guard let request = storyDirectVideoQueueState.begin(
            ownerUserID: owner,
            accountSessionID: session,
            projectID: plan.projectID,
            queueID: plan.queueID
        ) else { return }
        errorMessage = nil
        let token = session
        let queue = storyDirectVideoQueue
        // Core owns target ordering and advances only after the current generation Task ends.
        // This preserves actual-tail continuity without making the whole workspace busy.
        videoBatchTask = Task { [weak self, queue] in
            let result: Result<Void, Error>
            do {
                try await queue.execute(
                    plan,
                    currentProject: { [weak self] in
                        guard let self,
                              self.storyDirectVideoQueueState.accepts(
                                request,
                                ownerUserID: self.owner,
                                accountSessionID: token,
                                projectID: plan.projectID
                              ) else { return nil }
                        return self.projects.first(where: { $0.id == plan.projectID })
                    },
                    continuationCheck: { [weak self] in
                        try Task.checkCancellation()
                        guard let self,
                              self.storyDirectVideoQueueState.accepts(
                                request,
                                ownerUserID: self.owner,
                                accountSessionID: token,
                                projectID: plan.projectID
                              ) else { throw CancellationError() }
                    },
                    executeStep: { [weak self] latest, step in
                        let generation: Task<Void, Never>? = {
                            guard let self,
                                  self.storyDirectVideoQueueState.accepts(
                                    request,
                                    ownerUserID: self.owner,
                                    accountSessionID: token,
                                    projectID: step.projectID
                                  ) else { return nil }
                            let key = VideoGenerationKey(
                                projectID: step.projectID,
                                segmentID: step.segmentID
                            )
                            self.startVideoGeneration(
                                project: latest,
                                segmentID: step.segmentID,
                                model: step.model,
                                size: step.size,
                                owner: owner,
                                token: token,
                                userIdeas: step.userIdeas
                            )
                            return self.videoGenerationTasks[key]
                        }()
                        guard let generation else {
                            throw StoryDirectVideoQueueError.targetUnavailable
                        }
                        await generation.value
                    }
                )
                result = .success(())
            } catch {
                result = .failure(error)
            }
            guard let self else { return }
            let accepted = self.storyDirectVideoQueueState.accepts(
                request,
                ownerUserID: self.owner,
                accountSessionID: token,
                projectID: plan.projectID
            )
            _ = self.storyDirectVideoQueueState.finish(request)
            guard accepted else { return }
            self.videoBatchTask = nil
            if case .failure(let error) = result,
               !(error is CancellationError),
               !Task.isCancelled {
                self.errorMessage = error.localizedDescription
            }
        }
    }

    /// Keeps the completed cut in creation history and reopens the segment controls without
    /// submitting a billable request. The user can then change either frame before generating.
    func prepareCompletedVideoForEditing(_ segmentID: String) {
        guard var next = project,
              let index = next.segments.firstIndex(where: { $0.id == segmentID }),
              next.segments[index].video != nil else { return }
        next.segments[index].archiveCompletedVideoForRegeneration()
        runCommit("保留旧视频并开放重新制作", project: next)
    }

    /// Regenerates one completed segment only after the user confirms the charge in the UI.
    /// The old cut is archived before the provider request, so failure never destroys it.
    func regenerateCompletedVideo(_ segmentID: String, availableModels: [MediaGenerationModel],
                                  useOriginalVideo: Bool = true, userIdeas: String = "") {
        guard !isBusy, !isLoading, videoBatchTask == nil, let owner, let project else { return }
        var next = project
        guard let index = next.segments.firstIndex(where: { $0.id == segmentID }),
              next.segments[index].video != nil else { return }
        guard let model = availableModels.first(where: { $0.id == next.models.videoModelID }),
              model.enabled, model.hasAPIKey else {
            errorMessage = StoryError.missingModel.localizedDescription
            return
        }
        if useOriginalVideo {
            guard model.supportsVideoReference else {
                errorMessage = "当前视频模型不支持参考原视频重新生成。"
                return
            }
            guard !StoryGenerationContext.normalizedUserIdeas(userIdeas).isEmpty else {
                errorMessage = "请先填写原视频哪里需要调整。"
                return
            }
        }
        next.segments[index].archiveCompletedVideoForRegeneration()
        if useOriginalVideo {
            next.segments[index].videoGuidanceMode = .sourceVideo
        } else if next.segments[index].videoGuidanceMode == .sourceVideo {
            next.segments[index].videoGuidanceMode = .firstFrame
        }
        guard next.segments[index].isReady else {
            errorMessage = StoryError.invalidPlan.localizedDescription
            return
        }
        let profile = VideoGenerationProfile(modelName: model.modelName)
        guard profile.durations.contains(next.segments[index].seconds), let size = profile.sizes.first else {
            errorMessage = StoryError.unsupportedDuration.localizedDescription
            return
        }
        let token = session
        let projectID = next.id
        let archiveCommand: StoryProjectEditCommand
        do {
            archiveCommand = try .init(
                base: project,
                project: next,
                preservingVideoSegmentIDs: []
            )
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        guard let archiveRequest = projectMutationState.begin(
            ownerUserID: owner,
            accountSessionID: token,
            kind: .edit(
                projectID: archiveCommand.projectID,
                baseFingerprint: archiveCommand.baseFingerprint
            )
        ) else { return }
        let projectMutation = projectMutation
        errorMessage = nil
        selectedSegments = [segmentID]
        videoBatchTask = Task { [weak self, projectMutation] in
            let saved: Result<StoryProject, Error>
            do {
                saved = .success(try await projectMutation.persistEdit(
                    archiveCommand,
                    ownerUserID: owner
                ))
            } catch {
                saved = .failure(error)
            }
            let generation: Task<Void, Never>? = {
                guard let self else { return nil }
                let accepted = self.projectMutationState.accepts(
                    archiveRequest,
                    ownerUserID: self.owner,
                    accountSessionID: token
                )
                _ = self.projectMutationState.finish(archiveRequest)
                guard accepted, !Task.isCancelled else { return nil }
                switch saved {
                case .success(let latest):
                    self.publishProject(latest, token: token)
                    self.startVideoGeneration(
                        project: latest, segmentID: segmentID, model: model, size: size,
                        owner: owner, token: token, userIdeas: userIdeas
                    )
                case .failure(let error):
                    self.errorMessage = error.localizedDescription
                    self.videoBatchTask = nil
                    return nil
                }
                let key = VideoGenerationKey(projectID: projectID, segmentID: segmentID)
                return self.videoGenerationTasks[key]
            }()
            guard let generation else { return }
            await generation.value
            guard let self, self.session == token else { return }
            self.videoBatchTask = nil
        }
    }

    func startVideoGeneration(project: StoryProject, segmentID: String,
                                      model: MediaGenerationModel, size: String,
                                      owner: String, token: UUID, userIdeas: String = "") {
        do {
            let command = try storyVideoGeneration.prepareStart(
                ownerUserID: owner,
                project: project,
                segmentID: segmentID,
                model: model,
                size: size,
                userIdeas: userIdeas
            )
            launchVideoGeneration(command: command, owner: owner, token: token)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func launchVideoGeneration(
        command: StoryVideoGenerationCommand,
        owner: String,
        token: UUID
    ) {
        let key = VideoGenerationKey(
            projectID: command.projectID,
            segmentID: command.segmentID
        )
        guard !activeVideoGenerations.contains(key) else { return }
        guard let request = storyVideoGenerationState.begin(
            ownerUserID: owner,
            accountSessionID: token,
            command: command
        ) else { return }
        activeVideoGenerations.insert(key)
        generationActivity.setVideoProgress(.init(status: "preparing"), for: key)
        let storyVideoGeneration = storyVideoGeneration
        videoGenerationTasks[key] = Task { [weak self, storyVideoGeneration] in
            let result: Result<StoryVideoGenerationOutcome, Error>
            do {
                result = .success(try await storyVideoGeneration.execute(
                    ownerUserID: owner,
                    accountSessionID: token,
                    command: command,
                    continuationCheck: { [weak self] in
                        guard let self else { throw CancellationError() }
                        try await self.checkVideoGeneration(request, session: token)
                    },
                    onEvent: { [weak self] event in
                        await self?.receiveVideoGenerationEvent(
                            event,
                            request: request,
                            key: key,
                            token: token
                        )
                    }
                ))
            } catch {
                result = .failure(error)
            }
            guard let self,
                  self.storyVideoGenerationState.accepts(
                    request,
                    ownerUserID: self.owner,
                    accountSessionID: token
                  ),
                  self.storyVideoGenerationState.finish(request) else { return }
            self.activeVideoGenerations.remove(key)
            self.generationActivity.setVideoProgress(nil, for: key)
            self.videoGenerationTasks[key] = nil
            switch result {
            case .success(let outcome):
                self.publishProject(outcome.project, token: token)
                if outcome.project.segments.first(where: { $0.id == key.segmentID })?.video != nil {
                    self.selectedSegments.remove(key.segmentID)
                }
            case .failure(let error):
                guard !(error is CancellationError), !Task.isCancelled else { return }
                self.errorMessage = error.localizedDescription
            }
        }
    }

    private func checkVideoGeneration(
        _ request: StoryVideoGenerationToken,
        session token: UUID
    ) throws {
        try check(token)
        guard storyVideoGenerationState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: token
        ) else { throw CancellationError() }
    }

    private func receiveVideoGenerationEvent(
        _ event: StoryVideoGenerationEvent,
        request: StoryVideoGenerationToken,
        key: VideoGenerationKey,
        token: UUID
    ) {
        guard storyVideoGenerationState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: token
        ) else { return }
        if shouldPublishVideoGenerationProject(event.project, key: key) {
            publishProject(event.project, token: token)
        }
        generationActivity.setVideoProgress(.init(
            status: event.progress.status,
            percent: event.progress.percent
        ), for: key)
        if let error = event.error { errorMessage = error }
    }

    /// Status and percentage callbacks are already durable in the Story store and rendered
    /// by `generationActivity`. Only publish identity-bearing transitions to the large project
    /// graph; otherwise every provider poll would invalidate the entire workbench.
    private func shouldPublishVideoGenerationProject(
        _ incoming: StoryProject,
        key: VideoGenerationKey
    ) -> Bool {
        guard let current = projects.first(where: { $0.id == key.projectID }),
              let currentSegment = current.segments.first(where: { $0.id == key.segmentID }),
              let incomingSegment = incoming.segments.first(where: { $0.id == key.segmentID }) else {
            return true
        }
        return currentSegment.attempt?.id != incomingSegment.attempt?.id
            || currentSegment.attempt?.jobID != incomingSegment.attempt?.jobID
            || currentSegment.video != incomingSegment.video
            || currentSegment.error != incomingSegment.error
    }

    func resumeVideo(_ id: String) {
        guard !isBusy, !isLoading, let owner, let project else { return }
        do {
            let command = try storyVideoGeneration.prepareResume(
                ownerUserID: owner,
                project: project,
                segmentID: id
            )
            errorMessage = nil
            launchVideoGeneration(command: command, owner: owner, token: session)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Only called after an explicit user confirmation, never by the planning model.
    func allowRetryAfterVerification(_ id: String) {
        guard var next = project, let index = next.segments.firstIndex(where: { $0.id == id }),
              next.segments[index].video == nil, let attempt = next.segments[index].attempt else { return }
        next.segments[index].previousAttempts.append(attempt)
        next.segments[index].attempt = nil
        next.segments[index].error = nil
        runCommit("保留旧任务记录并允许手动重试", project: next)
    }

    func mediaAsset(_ image: StoryImage, projectID: UUID) -> GeneratedMediaAsset? {
        guard let owner else { return nil }
        return try? storyMediaAccess.imageAsset(
            ownerUserID: owner,
            projectID: projectID,
            image: image
        )
    }
    func videoURL(_ video: StoryVideo, projectID: UUID) -> URL? {
        guard let owner else { return nil }
        return try? storyMediaAccess.videoURL(
            ownerUserID: owner,
            projectID: projectID,
            video: video
        )
    }
    func reextractVideoLastFrame(_ segmentID: String) {
        guard let owner, let project else { return }
        let command: StoryVideoTailExtractionCommand
        do {
            command = try storyVideoTailExtraction.prepare(
                ownerUserID: owner,
                project: project,
                segmentID: segmentID
            )
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        let key = VideoGenerationKey(
            projectID: command.projectID,
            segmentID: command.segmentID
        )
        guard !activeVideoFrameExtractions.contains(key) else { return }
        let token = session
        guard let request = storyVideoTailExtractionState.begin(
            command: command,
            accountSessionID: token
        ) else { return }
        let storyVideoTailExtraction = storyVideoTailExtraction
        activeVideoFrameExtractions.insert(key)
        errorMessage = nil
        videoFrameExtractionTasks[key] = Task { [weak self, storyVideoTailExtraction] in
            let result: Result<StoryProject, Error>
            do {
                result = .success(try await storyVideoTailExtraction.extract(command))
            } catch {
                result = .failure(error)
            }
            guard let self,
                  self.storyVideoTailExtractionState.accepts(
                    request,
                    ownerUserID: self.owner,
                    accountSessionID: token
                  ),
                  self.storyVideoTailExtractionState.finish(request) else { return }
            self.activeVideoFrameExtractions.remove(key)
            self.videoFrameExtractionTasks[key] = nil
            switch result {
            case .success(let project):
                self.publishProject(project, token: token)
            case .failure(let error):
                if error is CancellationError { return }
                self.errorMessage = "无法从当前视频提取末帧：\(error.localizedDescription)"
            }
        }
    }
}
