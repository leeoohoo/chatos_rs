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
            guard let self else { return }
            defer { if self.session == token { self.videoBatchTask = nil } }
            for id in ids {
                guard !Task.isCancelled, self.session == token,
                      let latest = self.projects.first(where: { $0.id == project.id }) else { return }
                self.startVideoGeneration(
                    project: latest, segmentID: id, model: model, size: size,
                    owner: owner, token: token, userIdeas: userIdeas
                )
                let key = VideoGenerationKey(projectID: project.id, segmentID: id)
                if let generation = self.videoGenerationTasks[key] { await generation.value }
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
        run("保留旧视频并开放重新制作") { owner, token in
            try await self.commit(next, owner: owner, token: token)
        }
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
        errorMessage = nil
        selectedSegments = [segmentID]
        videoBatchTask = Task { [weak self] in
            guard let self else { return }
            defer { if self.session == token { self.videoBatchTask = nil } }
            do {
                try await self.commit(next, owner: owner, token: token)
                guard !Task.isCancelled, self.session == token,
                      let latest = self.projects.first(where: { $0.id == projectID }) else { return }
                self.startVideoGeneration(
                    project: latest, segmentID: segmentID, model: model, size: size,
                    owner: owner, token: token, userIdeas: userIdeas
                )
                let key = VideoGenerationKey(projectID: projectID, segmentID: segmentID)
                if let generation = self.videoGenerationTasks[key] { await generation.value }
            } catch is CancellationError {
                return
            } catch {
                if self.session == token { self.errorMessage = error.localizedDescription }
            }
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
        launchVideoGeneration(key: key, attemptID: attempt.id, owner: owner, token: token) { service in
            // The store actor merges this intent into the latest manifest before the billable POST.
            let intent = try await self.store.beginVideoGeneration(
                projectID: project.id, segmentID: segmentID, attempt: attempt, owner: owner
            )
            try self.check(token)
            self.publishProject(intent.project, token: token)

            var firstFrameInput: ImageGenerationInputImage?
            var lastFrameInput: ImageGenerationInputImage?
            var referenceVideoInput: VideoGenerationInputVideo?
            var referencePurpose: VideoGenerationReferencePurpose = .reference
            switch intent.segment.videoGuidanceMode {
            case .firstFrame, .firstAndLastFrames:
                guard let frame = intent.segment.firstFrame else { throw StoryError.invalidPlan }
                let firstURL = try self.store.fileURL(frame.filename, projectID: project.id, owner: owner)
                let firstData = try await MediaStudioImageLoader.data(for: .init(
                    id: frame.id.uuidString, mimeType: frame.mimeType, url: firstURL
                ))
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
                let previousURL = try self.store.fileURL(
                    referenceVideo.filename, projectID: project.id, owner: owner
                )
                let previousData = try await Task.detached(priority: .userInitiated) {
                    try Data(contentsOf: previousURL, options: .mappedIfSafe)
                }.value
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
                let lastURL = try self.store.fileURL(lastFrame.filename, projectID: project.id, owner: owner)
                let lastData = try await MediaStudioImageLoader.data(for: .init(
                    id: lastFrame.id.uuidString, mimeType: lastFrame.mimeType, url: lastURL
                ))
                lastFrameInput = .init(
                    name: "last-frame.png", mimeType: lastFrame.mimeType,
                    base64Data: lastData.base64EncodedString()
                )
            }
            try self.check(token)
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
            try await self.executeParallelVideo(
                request, jobID: nil, projectID: project.id, segmentID: segmentID,
                attemptID: attempt.id, owner: owner, token: token, service: service
            )
        }
    }

    func launchVideoGeneration(
        key: VideoGenerationKey,
        attemptID: UUID,
        owner: String,
        token: UUID,
        operation: @escaping @MainActor (any MediaGenerationServicing) async throws -> Void
    ) {
        guard !activeVideoGenerations.contains(key) else { return }
        activeVideoGenerations.insert(key)
        videoGenerationProgress[key] = .init(status: "preparing")
        videoGenerationTasks[key] = Task { [weak self] in
            guard let self else { return }
            defer {
                if self.session == token {
                    self.activeVideoGenerations.remove(key)
                    self.videoGenerationProgress[key] = nil
                    self.videoGenerationTasks[key] = nil
                }
            }
            do {
                let service = try await self.boundMedia(token)
                try await operation(service)
            } catch is CancellationError {
                // A submitted attempt stays recoverable and is never automatically re-posted.
            } catch {
                guard self.session == token else { return }
                do {
                    let failed: StoryProject
                    if let failure = error as? any MediaGenerationSubmissionFailure,
                       !failure.requestMayHaveBeenSubmitted {
                        failed = try await self.store.failVideoGenerationBeforeSubmission(
                            error.localizedDescription, projectID: key.projectID,
                            segmentID: key.segmentID, attemptID: attemptID, owner: owner
                        )
                    } else {
                        failed = try await self.store.failVideoGeneration(
                            error.localizedDescription, projectID: key.projectID,
                            segmentID: key.segmentID, attemptID: attemptID, owner: owner
                        )
                    }
                    self.publishProject(failed, token: token)
                } catch {
                    self.errorMessage = error.localizedDescription
                }
            }
        }
    }

    func executeParallelVideo(
        _ request: VideoGenerationRequest,
        jobID: String?,
        projectID: UUID,
        segmentID: String,
        attemptID: UUID,
        owner: String,
        token: UUID,
        service: any MediaGenerationServicing
    ) async throws {
        let key = VideoGenerationKey(projectID: projectID, segmentID: segmentID)
        videoGenerationProgress[key] = .init(
            status: jobID == nil ? "submitting" : "checking",
            jobID: jobID
        )
        let callback: @Sendable (VideoGenerationProgress) async -> Void = { [weak self] value in
            await self?.recordParallelVideoProgress(
                value, projectID: projectID, segmentID: segmentID,
                attemptID: attemptID, owner: owner, token: token
            )
        }
        let result: VideoGenerationResult
        if let jobID {
            guard let resumable = service as? any ResumableVideoGenerationServicing else {
                throw StoryError.unavailable
            }
            result = try await resumable.resumeVideo(request, jobID: jobID, progress: callback)
        } else {
            result = try await service.generateVideo(request, progress: callback)
        }
        try check(token)
        let completed = try await store.completeVideoGeneration(
            result, projectID: projectID, segmentID: segmentID,
            attemptID: attemptID, owner: owner
        )
        var finalProject = completed.0
        do {
            let videoURL = try store.fileURL(completed.1.filename, projectID: projectID, owner: owner)
            let finalFrame = try await StoryVideoFrameExtractor.lastFramePNG(from: videoURL)
            try check(token)
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
        try check(token)
        publishProject(finalProject, token: token)
        selectedSegments.remove(segmentID)
    }

    func recordParallelVideoProgress(
        _ value: VideoGenerationProgress,
        projectID: UUID,
        segmentID: String,
        attemptID: UUID,
        owner: String,
        token: UUID
    ) async {
        guard session == token else { return }
        let key = VideoGenerationKey(projectID: projectID, segmentID: segmentID)
        videoGenerationProgress[key] = value
        do {
            let updated = try await store.updateVideoGenerationProgress(
                value, projectID: projectID, segmentID: segmentID,
                attemptID: attemptID, owner: owner
            )
            try check(token)
            publishProject(updated, token: token)
        } catch {
            guard session == token else { return }
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
        launchVideoGeneration(key: key, attemptID: attempt.id, owner: owner, token: token) { service in
            let request = VideoGenerationRequest(
                modelConfigID: attempt.modelConfigID, prompt: attempt.prompt,
                size: attempt.size, seconds: attempt.seconds, ratio: attempt.ratio
            )
            try await self.executeParallelVideo(
                request, jobID: jobID, projectID: project.id, segmentID: id,
                attemptID: attempt.id, owner: owner, token: token, service: service
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
        run("保留旧任务记录并允许手动重试") { owner, token in
            try await self.commit(next, owner: owner, token: token)
        }
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
        activeVideoFrameExtractions.insert(key)
        errorMessage = nil
        videoFrameExtractionTasks[key] = Task { [weak self] in
            guard let self else { return }
            defer {
                if self.session == token {
                    self.activeVideoFrameExtractions.remove(key)
                    self.videoFrameExtractionTasks[key] = nil
                }
            }
            do {
                let url = try self.store.fileURL(video.filename, projectID: project.id, owner: owner)
                let png = try await StoryVideoFrameExtractor.lastFramePNG(from: url)
                try self.check(token)
                let applied = try await self.store.applyActualVideoLastFrame(
                    png, projectID: project.id, segmentID: segmentID,
                    videoJobID: video.jobID, owner: owner, forceNew: true
                )
                try self.check(token)
                self.publishProject(applied.0, token: token)
            } catch is CancellationError {
            } catch {
                guard self.session == token else { return }
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
