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
        guard let model = availableModels.first(where: { $0.id == project.models.videoModelID }) else {
            errorMessage = StoryError.missingModel.localizedDescription
            return
        }
        let profile = VideoGenerationProfile(modelName: model.modelName)
        let unsupported = project.segments.filter {
            ids.contains($0.id) && !profile.durations.contains($0.seconds)
        }
        guard unsupported.isEmpty, let size = profile.sizes.first else {
            let durations = profile.durations.map(String.init).joined(separator: "、")
            errorMessage = "当前视频模型不支持所选分段时长。模型支持：\(durations) 秒；不支持："
                + unsupported.map { "\($0.title)（\($0.seconds)秒）" }.joined(separator: "、")
            return
        }
        let referenceVideoSegments = project.segments.filter {
            ids.contains($0.id)
                && ($0.videoGuidanceMode == .previousVideo || $0.videoGuidanceMode == .sourceVideo)
        }
        let unavailablePreviousVideo = project.segments.enumerated().compactMap { index, segment -> String? in
            guard referenceVideoSegments.contains(where: { $0.id == segment.id }),
                  segment.videoGuidanceMode == .previousVideo else { return nil }
            return index > 0 && project.segments[index - 1].video != nil ? nil : segment.title
        }
        let unavailableSourceVideo = referenceVideoSegments.filter {
            $0.videoGuidanceMode == .sourceVideo && $0.archivedVideos.last == nil
        }
        guard referenceVideoSegments.isEmpty || model.supportsVideoReference else {
            errorMessage = "当前视频模型不支持视频参考输入。"
            return
        }
        guard unavailablePreviousVideo.isEmpty else {
            errorMessage = "请先完成上一段视频，再为以下分段选择“上一段视频”："
                + unavailablePreviousVideo.joined(separator: "、")
            return
        }
        guard unavailableSourceVideo.isEmpty else {
            errorMessage = "找不到要重做的原视频："
                + unavailableSourceVideo.map(\.title).joined(separator: "、")
            return
        }
        errorMessage = nil
        let token = session
        // Adjacent videos must be submitted in story order: the preceding completed clip's
        // decoded final frame becomes the next request's first frame.
        videoBatchTask = Task { [weak self] in
            for id in ids {
                let key = VideoGenerationKey(projectID: project.id, segmentID: id)
                guard !Task.isCancelled else { return }
                let generation: Task<Void, Never>? = {
                    guard let self, self.session == token,
                          let latest = self.projects.first(where: { $0.id == project.id }) else {
                        return nil
                    }
                    self.startVideoGeneration(
                        project: latest, segmentID: id, model: model, size: size,
                        owner: owner, token: token, userIdeas: userIdeas
                    )
                    return self.videoGenerationTasks[key]
                }()
                guard let generation else {
                    if let self, self.session == token { self.videoBatchTask = nil }
                    return
                }
                await generation.value
            }
            guard let self, self.session == token else { return }
            self.videoBatchTask = nil
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
        guard !isBusy, !isLoading, videoBatchTask == nil, let owner, var next = project,
              let index = next.segments.firstIndex(where: { $0.id == segmentID }),
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
        let store = store
        errorMessage = nil
        selectedSegments = [segmentID]
        videoBatchTask = Task { [weak self, store] in
            let saved: Result<StoryProject, Error>
            do {
                var archived = next
                StoryContinuityContext.reconcileInheritedFirstFrames(&archived)
                archived.updatedAt = Date()
                saved = .success(try await store.save(
                    archived, owner: owner, preservingVideoSegmentIDs: []
                ))
            } catch {
                saved = .failure(error)
            }
            guard !Task.isCancelled else { return }
            let generation: Task<Void, Never>? = {
                guard let self, self.session == token else { return nil }
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
        guard let segment = project.segments.first(where: { $0.id == segmentID }),
              segment.isReady else { return }
        let key = VideoGenerationKey(projectID: project.id, segmentID: segmentID)
        guard !activeVideoGenerations.contains(key) else { return }
        let prompt: String
        do { prompt = try StoryGenerationContext.videoPrompt(project, segment: segment, userIdeas: userIdeas) }
        catch { errorMessage = error.localizedDescription; return }
        let attempt = StoryVideoAttempt(
            modelConfigID: project.models.videoModelID,
            prompt: prompt,
            size: size,
            ratio: project.ratio,
            seconds: segment.seconds
        )
        let store = store
        launchVideoGeneration(key: key, attemptID: attempt.id, owner: owner, token: token) { [weak self, store] service in
            // The store actor merges this intent into the latest manifest before the billable POST.
            let intent = try await store.beginVideoGeneration(
                projectID: project.id, segmentID: segmentID, attempt: attempt, owner: owner
            )
            await self?.publishProject(intent.project, token: token)
            try Task.checkCancellation()

            var firstFrameInput: ImageGenerationInputImage?
            var lastFrameInput: ImageGenerationInputImage?
            var referenceVideoInput: VideoGenerationInputVideo?
            var referencePurpose: VideoGenerationReferencePurpose = .reference
            switch intent.segment.videoGuidanceMode {
            case .firstFrame, .firstAndLastFrames:
                guard let frame = intent.segment.firstFrame else { throw StoryError.invalidPlan }
                let firstURL = try store.fileURL(frame.filename, projectID: project.id, owner: owner)
                let firstData = try await MediaStudioImageLoader.data(for: .init(
                    id: frame.id.uuidString, mimeType: frame.mimeType, url: firstURL
                ))
                try Task.checkCancellation()
                firstFrameInput = .init(
                    name: "first-frame.png", mimeType: frame.mimeType,
                    base64Data: firstData.base64EncodedString()
                )
            case .previousVideo, .sourceVideo:
                guard model.supportsVideoReference,
                      let index = intent.project.segments.firstIndex(where: { $0.id == segmentID }) else {
                    throw StoryError.invalidPlan
                }
                let referenceVideo: StoryVideo?
                if intent.segment.videoGuidanceMode == .previousVideo {
                    referenceVideo = index > 0 ? intent.project.segments[index - 1].video : nil
                } else {
                    referenceVideo = intent.segment.archivedVideos.last
                }
                guard let referenceVideo else { throw StoryError.invalidPlan }
                let previousURL = try store.fileURL(
                    referenceVideo.filename, projectID: project.id, owner: owner
                )
                let previousData = try await AppCancellableDetachedWork.run {
                    try AppBoundedFileReader.read(
                        previousURL,
                        maximumBytes: 512 * 1_024 * 1_024
                    )
                }
                try Task.checkCancellation()
                referenceVideoInput = .init(
                    name: intent.segment.videoGuidanceMode == .previousVideo
                        ? "previous-segment.mp4" : "source-video.mp4",
                    mimeType: "video/mp4",
                    base64Data: previousData.base64EncodedString()
                )
                referencePurpose = intent.segment.videoGuidanceMode == .previousVideo ? .extend : .edit
            }
            if model.supportsVideoLastFrame,
               intent.segment.videoGuidanceMode == .firstAndLastFrames,
               let lastFrame = intent.segment.lastFrame {
                let lastURL = try store.fileURL(lastFrame.filename, projectID: project.id, owner: owner)
                let lastData = try await MediaStudioImageLoader.data(for: .init(
                    id: lastFrame.id.uuidString, mimeType: lastFrame.mimeType, url: lastURL
                ))
                try Task.checkCancellation()
                lastFrameInput = .init(
                    name: "last-frame.png", mimeType: lastFrame.mimeType,
                    base64Data: lastData.base64EncodedString()
                )
            }
            try Task.checkCancellation()
            let request = VideoGenerationRequest(
                modelConfigID: attempt.modelConfigID,
                prompt: attempt.prompt,
                size: attempt.size,
                seconds: attempt.seconds,
                inputImage: firstFrameInput,
                lastFrameImage: lastFrameInput,
                referenceVideo: referenceVideoInput,
                referencePurpose: referencePurpose,
                ratio: attempt.ratio
            )
            await self?.setParallelVideoProgress(
                .init(status: "submitting"), key: key, token: token
            )
            return try await Self.executeParallelVideo(
                request, jobID: nil, projectID: project.id, segmentID: segmentID,
                attemptID: attempt.id, owner: owner, store: store, service: service,
                progress: { [weak self, store] value in
                    await self?.setParallelVideoProgress(value, key: key, token: token)
                    let persisted: Result<StoryProject, Error>
                    do {
                        persisted = .success(try await store.updateVideoGenerationProgress(
                            value, projectID: project.id, segmentID: segmentID,
                            attemptID: attempt.id, owner: owner
                        ))
                    } catch {
                        persisted = .failure(error)
                    }
                    await self?.applyParallelVideoProgress(
                        persisted, value: value, key: key, token: token
                    )
                }
            )
        }
    }

    func launchVideoGeneration(
        key: VideoGenerationKey,
        attemptID: UUID,
        owner: String,
        token: UUID,
        operation: @escaping @Sendable (any MediaGenerationServicing) async throws -> StoryProject
    ) {
        guard !activeVideoGenerations.contains(key) else { return }
        activeVideoGenerations.insert(key)
        videoGenerationProgress[key] = .init(status: "preparing")
        let store = store
        let media = media
        videoGenerationTasks[key] = Task { [weak self, store, media] in
            let outcome: Result<StoryProject, Error>
            do {
                let service: any MediaGenerationServicing
                if let bindable = media as? any SessionBoundMediaGenerationServicing {
                    service = try await bindable.boundToCurrentSession()
                } else {
                    service = media
                }
                try Task.checkCancellation()
                outcome = .success(try await operation(service))
            } catch {
                if error is CancellationError || Task.isCancelled {
                    outcome = .failure(CancellationError())
                } else {
                    do {
                        let failed: StoryProject
                        if let failure = error as? any MediaGenerationSubmissionFailure,
                           !failure.requestMayHaveBeenSubmitted {
                            failed = try await store.failVideoGenerationBeforeSubmission(
                                error.localizedDescription, projectID: key.projectID,
                                segmentID: key.segmentID, attemptID: attemptID, owner: owner
                            )
                        } else {
                            failed = try await store.failVideoGeneration(
                                error.localizedDescription, projectID: key.projectID,
                                segmentID: key.segmentID, attemptID: attemptID, owner: owner
                            )
                        }
                        outcome = .success(failed)
                    } catch {
                        outcome = .failure(error)
                    }
                }
            }
            guard let self, self.session == token else { return }
            self.activeVideoGenerations.remove(key)
            self.videoGenerationProgress[key] = nil
            self.videoGenerationTasks[key] = nil
            switch outcome {
            case .success(let project):
                self.publishProject(project, token: token)
                if project.segments.first(where: { $0.id == key.segmentID })?.video != nil {
                    self.selectedSegments.remove(key.segmentID)
                }
            case .failure(let error):
                guard !(error is CancellationError), !Task.isCancelled else { return }
                self.errorMessage = error.localizedDescription
            }
        }
    }

    nonisolated static func executeParallelVideo(
        _ request: VideoGenerationRequest,
        jobID: String?,
        projectID: UUID,
        segmentID: String,
        attemptID: UUID,
        owner: String,
        store: StoryProjectStore,
        service: any MediaGenerationServicing,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> StoryProject {
        try Task.checkCancellation()
        let result: VideoGenerationResult
        if let jobID {
            guard let resumable = service as? any ResumableVideoGenerationServicing else {
                throw StoryError.unavailable
            }
            result = try await resumable.resumeVideo(request, jobID: jobID, progress: progress)
        } else {
            result = try await service.generateVideo(request, progress: progress)
        }
        try Task.checkCancellation()
        let completed = try await store.completeVideoGeneration(
            result, projectID: projectID, segmentID: segmentID,
            attemptID: attemptID, owner: owner
        )
        var finalProject = completed.0
        do {
            let videoURL = try store.fileURL(completed.1.filename, projectID: projectID, owner: owner)
            let finalFrame = try await StoryVideoFrameExtractor.lastFramePNG(from: videoURL)
            try Task.checkCancellation()
            let applied = try await store.applyActualVideoLastFrame(
                finalFrame, projectID: projectID, segmentID: segmentID,
                videoJobID: completed.1.jobID, owner: owner
            )
            finalProject = applied.0
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            // The provider video is already complete and remains usable. Frame extraction is
            // a local continuity enhancement, so a malformed/unsupported file is non-fatal.
        }
        try Task.checkCancellation()
        return finalProject
    }

    func setParallelVideoProgress(
        _ value: VideoGenerationProgress,
        key: VideoGenerationKey,
        token: UUID
    ) {
        guard session == token else { return }
        videoGenerationProgress[key] = value
    }

    func applyParallelVideoProgress(
        _ result: Result<StoryProject, Error>,
        value: VideoGenerationProgress,
        key: VideoGenerationKey,
        token: UUID
    ) {
        guard session == token else { return }
        switch result {
        case .success(let updated):
            publishProject(updated, token: token)
        case .failure(let error):
            errorMessage = "保存任务 ID 失败，请保留任务 ID \(value.jobID ?? "未知")，勿重复提交：\(error.localizedDescription)"
            videoGenerationTasks[key]?.cancel()
        }
    }

    func resumeVideo(_ id: String) {
        guard !isBusy, !isLoading, let owner, let project,
              let segment = project.segments.first(where: { $0.id == id }),
              segment.video == nil, let attempt = segment.attempt else { return }
        guard let jobID = attempt.jobID else {
            errorMessage = StoryError.unresolvedSubmission.localizedDescription
            return
        }
        let key = VideoGenerationKey(projectID: project.id, segmentID: id)
        guard !activeVideoGenerations.contains(key) else { return }
        errorMessage = nil
        let token = session
        let store = store
        launchVideoGeneration(key: key, attemptID: attempt.id, owner: owner, token: token) { [weak self, store] service in
            let request = VideoGenerationRequest(
                modelConfigID: attempt.modelConfigID, prompt: attempt.prompt,
                size: attempt.size, seconds: attempt.seconds, ratio: attempt.ratio
            )
            await self?.setParallelVideoProgress(
                .init(status: "checking", jobID: jobID), key: key, token: token
            )
            return try await Self.executeParallelVideo(
                request, jobID: jobID, projectID: project.id, segmentID: id,
                attemptID: attempt.id, owner: owner, store: store, service: service,
                progress: { [weak self, store] value in
                    await self?.setParallelVideoProgress(value, key: key, token: token)
                    let persisted: Result<StoryProject, Error>
                    do {
                        persisted = .success(try await store.updateVideoGenerationProgress(
                            value, projectID: project.id, segmentID: id,
                            attemptID: attempt.id, owner: owner
                        ))
                    } catch {
                        persisted = .failure(error)
                    }
                    await self?.applyParallelVideoProgress(
                        persisted, value: value, key: key, token: token
                    )
                }
            )
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
        guard let owner, let url = try? store.fileURL(image.filename, projectID: projectID, owner: owner) else { return nil }
        return .init(id: image.id.uuidString, mimeType: image.mimeType, url: url)
    }
    func videoURL(_ video: StoryVideo, projectID: UUID) -> URL? {
        guard let owner else { return nil }
        return try? store.fileURL(video.filename, projectID: projectID, owner: owner)
    }
    func reextractVideoLastFrame(_ segmentID: String) {
        guard let owner, let project,
              let segment = project.segments.first(where: { $0.id == segmentID }),
              let video = segment.video else { return }
        let key = VideoGenerationKey(projectID: project.id, segmentID: segmentID)
        guard !activeVideoFrameExtractions.contains(key) else { return }
        let token = session
        let store = store
        let filename = video.filename
        let videoJobID = video.jobID
        let projectID = project.id
        activeVideoFrameExtractions.insert(key)
        errorMessage = nil
        videoFrameExtractionTasks[key] = Task { [weak self, store] in
            let result: Result<StoryProject, Error>
            do {
                let url = try store.fileURL(filename, projectID: projectID, owner: owner)
                let png = try await StoryVideoFrameExtractor.lastFramePNG(from: url)
                try Task.checkCancellation()
                let applied = try await store.applyActualVideoLastFrame(
                    png, projectID: projectID, segmentID: segmentID,
                    videoJobID: videoJobID, owner: owner, forceNew: true
                )
                result = .success(applied.0)
            } catch {
                result = .failure(error)
            }
            guard let self, self.session == token, !Task.isCancelled else { return }
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
    func validateModels(_ selection: StoryModelSelection, available: [MediaGenerationModel]) throws {
        let ids = Set(available.filter { $0.enabled && $0.hasAPIKey }.map(\.id))
        guard [selection.textModelID, selection.imageModelID, selection.videoModelID].allSatisfy(ids.contains) else { throw StoryError.missingModel }
    }
    func modelSelectionWithCapabilities(
        _ selection: StoryModelSelection, available: [MediaGenerationModel]
    ) -> StoryModelSelection {
        var result = selection
        if let model = available.first(where: { $0.id == selection.videoModelID }) {
            result.supportedVideoDurations = VideoGenerationProfile(modelName: model.modelName).durations
        }
        return result
    }

}
