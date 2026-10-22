import AppKit
import ChatOSCore
import Foundation

/// A serial, deterministic queue. Waiting for a video never calls a text model.
actor StoryMediaBatchSession {
    private var state: StoryMediaBatch
    private let store: StoryProjectStore
    private let media: any MediaGenerationServicing
    private let videoJobStore: MediaStudioVideoJobStore
    private let accountSessionID: UUID
    private let check: @Sendable () async throws -> Void
    private let shouldPause: @Sendable () async -> Bool
    private let publish: @Sendable (StoryMediaBatch) async -> Void
    private let publishVideoProgress: @Sendable (
        StoryMediaBatch,
        String,
        VideoGenerationProgress
    ) async -> Void
    private var progressError: (any Error)?
    private var persistedProgressStatus: String?
    private var persistedProviderJobID: String?

    init(batch: StoryMediaBatch, store: StoryProjectStore, media: any MediaGenerationServicing,
         videoJobStore: MediaStudioVideoJobStore, accountSessionID: UUID,
         check: @escaping @Sendable () async throws -> Void = { try Task.checkCancellation() },
         shouldPause: @escaping @Sendable () async -> Bool = { false },
         publish: @escaping @Sendable (StoryMediaBatch) async -> Void = { _ in },
         publishVideoProgress: @escaping @Sendable (
             StoryMediaBatch,
             String,
             VideoGenerationProgress
         ) async -> Void = { _, _, _ in }) {
        self.state = batch; self.store = store; self.media = media
        self.videoJobStore = videoJobStore; self.accountSessionID = accountSessionID; self.check = check
        self.shouldPause = shouldPause; self.publish = publish
        self.publishVideoProgress = publishVideoProgress
    }

    func run() async throws -> StoryMediaBatch {
        try await check()
        guard !state.finished else { return state }
        try await store.commitMediaBatch(state)
        do {
            var started = state; started.status = .running; started.error = nil
            try await persist(started)
            for step in state.steps {
                try await check()
                if state.jobs[step.id]?.completed == true {
                    if step.kind == .videos { try await reconcileCompletedVideo(step) }
                    continue
                }
                if await shouldPause() {
                    var paused = state; paused.status = .paused; try await persist(paused); return state
                }
                if state.jobs[step.id] != nil, step.kind != .videos {
                    throw StoryError.unresolvedSubmission
                }
                try await generate(step)
            }
            var done = state; done.status = .completed; done.events.append(.init(detail: "本批制作完成"))
            try await persist(done)
        } catch {
            var stopped = state
            stopped.status = state.jobs.values.contains { !$0.completed } ? .needsReview : .paused
            stopped.error = error.localizedDescription
            stopped.events.append(.init(detail: error.localizedDescription))
            // Account cancellation must not publish into the new account. Durable intents
            // already exist; an interrupted status is inferred when this file is reopened.
            if error is CancellationError || Task.isCancelled { throw error }
            try await persist(stopped)
        }
        return state
    }

    private func generate(_ step: StoryMediaBatch.Step) async throws {
        try await check()
        let id = step.targetID; let key = step.id; let project = state.draft
        if step.kind == .frames,
           project.segments.first(where: { $0.id == id })?.firstFrame != nil {
            var next = state
            next.jobs[key] = .init(completed: true)
            next.events.append(.init(detail: "已承接上一段成片最后一帧作为首帧：\(id)，未调用图片模型"))
            try await persist(next)
            return
        }
        if step.kind == .videos {
            try await generateVideo(step)
            return
        }
        var request: ImageGenerationRequest
        if step.kind == .assets {
            guard let asset = project.resource(id: id) else { throw StoryError.invalidPlan }
            request = .init(modelConfigID: project.models.imageModelID,
                            prompt: StoryGenerationContext.assetPrompt(
                                project, resource: asset, userIdeas: state.userIdeas ?? ""
                            ), size: nil, count: 1)
        } else {
            guard let segment = project.segments.first(where: { $0.id == id }), segment.detail != nil else { throw StoryError.invalidPlan }
            var references: [ImageGenerationInputImage] = []
            for assetID in segment.resourceIDs {
                guard let asset = project.resource(id: assetID), let image = asset.confirmedImage else { throw StoryError.invalidPlan }
                references.append(try imageInput(image, name: asset.name + ".png"))
            }
            var previousTailReferenceIndex: Int?
            var currentFirstFrameReferenceIndex: Int?
            if step.kind == .frames,
               let previous = StoryContinuityContext.previousTail(project, segmentID: segment.id) {
                references.append(try imageInput(previous.image, name: "previous-segment-last-frame.png"))
                previousTailReferenceIndex = references.count
            } else if step.kind == .lastFrames, let firstFrame = segment.firstFrame {
                references.append(try imageInput(firstFrame, name: "current-segment-first-frame.png"))
                currentFirstFrameReferenceIndex = references.count
            }
            request = .init(modelConfigID: project.models.imageModelID,
                prompt: try StoryGenerationContext.framePrompt(project, segment: segment,
                                                               role: step.kind == .frames ? .first : .last,
                                                               referenceResourceIDs: segment.resourceIDs,
                                                               previousTailReferenceIndex: previousTailReferenceIndex,
                                                               currentFirstFrameReferenceIndex: currentFirstFrameReferenceIndex,
                                                               userIdeas: state.userIdeas ?? ""),
                size: nil, count: 1, referenceImages: references)
        }
        var intent = state; let job = StoryMediaBatch.Job(); intent.jobs[key] = job
        request.clientRequestID = job.intentID.uuidString
        request.projectID = project.id.uuidString
        request.resourceID = step.kind == .assets ? id : "\(id):\(step.kind == .frames ? StoryFrameRole.first.rawValue : StoryFrameRole.last.rawValue)"
        if step.kind == .assets, var resource = intent.draft.resource(id: id) {
            resource.media.generationAttemptID = job.intentID
            try intent.draft.replaceResource(resource)
        }
        if step.kind == .frames, let index = intent.draft.segments.firstIndex(where: { $0.id == id }) { intent.draft.segments[index].imageGenerationAttemptID = job.intentID }
        if step.kind == .lastFrames, let index = intent.draft.segments.firstIndex(where: { $0.id == id }) { intent.draft.segments[index].lastFrameGenerationAttemptID = job.intentID }
        intent.events.append(.init(detail: "生成图片：\(id)"))
        try await persist(intent); try await check()
        let result = try await media.generateImage(request)
        try await check()
        guard result.clientRequestID == nil || result.clientRequestID == request.clientRequestID,
              result.projectID == nil || result.projectID == request.projectID,
              result.resourceID == nil || result.resourceID == request.resourceID else { throw StoryError.invalidPlan }
        guard let image = result.images.first else { throw StoryError.unsafeFile }
        let pngFile = try await MediaStudioImageLoader.normalizedPNGFile(for: image)
        let stored = try await store.saveImage(pngFile, mimeType: "image/png", projectID: project.id, owner: state.owner,
                                               sourceResourceID: id, generationAttemptID: job.intentID,
                                               providerResultID: result.id, providerAssetID: image.id)
        var next = state; next.jobs[key]?.completed = true
        if step.kind == .assets, var resource = next.draft.resource(id: id) {
            resource.media.images.append(stored)
            resource.media.generationAttemptID = nil
            if state.kind == .pipeline { resource.media.confirmedImageID = stored.id }
            try next.draft.replaceResource(resource)
        } else if step.kind == .frames, let index = next.draft.segments.firstIndex(where: { $0.id == id }) {
            next.draft.segments[index].firstFrames.images.append(stored); next.draft.segments[index].imageGenerationAttemptID = nil
            if state.kind == .pipeline {
                next.draft.segments[index].confirmedFrameID = stored.id
                next.draft.segments[index].inheritedFirstFrameSourceSegmentID = nil
            }
        } else if step.kind == .lastFrames, let index = next.draft.segments.firstIndex(where: { $0.id == id }) {
            next.draft.segments[index].lastFrames.images.append(stored); next.draft.segments[index].lastFrameGenerationAttemptID = nil
            if state.kind == .pipeline { next.draft.segments[index].confirmedLastFrameID = stored.id }
        }
        StoryContinuityContext.reconcileInheritedFirstFrames(&next.draft)
        next.events.append(.init(detail: "图片已保存：\(id)"))
        try await persist(next)
    }

    private func generateVideo(_ step: StoryMediaBatch.Step) async throws {
        let project = state.draft
        guard let index = project.segments.firstIndex(where: { $0.id == step.targetID }),
              project.segments[index].detail != nil,
              let model = state.models.first(where: { $0.id == project.models.videoModelID }) else {
            throw StoryError.invalidPlan
        }
        let profile = VideoGenerationProfile(modelName: model.modelName)
        guard profile.durations.contains(project.segments[index].seconds),
              let size = profile.sizes.first else {
            throw StoryError.unsupportedDuration
        }

        let existingJob = state.jobs[step.id]
        let localJobID = existingJob?.intentID ?? UUID()
        let workload = MediaStudioVideoJobStore.Workload.storyBatchVideo(
            projectID: project.id,
            batchID: state.id,
            segmentID: step.targetID,
            attemptID: localJobID
        )
        var execution: MediaStudioVideoJobStore.Execution

        if existingJob?.videoJobRecordID == nil {
            let existingRecord = try await videoJobStore.snapshotIfPresent(
                id: localJobID,
                owner: state.owner
            )
            let request = try await makeVideoRequest(
                project: project,
                segmentIndex: index,
                model: model,
                size: size,
                includesInputs: existingJob == nil || existingRecord?.status == .prepared
            )
            if let existingRecord {
                guard existingRecord.workload == workload, !existingRecord.isActive else {
                    throw MediaStudioVideoJobStoreError.jobConflict
                }
                switch existingRecord.status {
                case .prepared:
                    guard existingJob?.jobID == nil else {
                        throw MediaStudioVideoJobStoreError.jobConflict
                    }
                    _ = try await videoJobStore.stagePrepared(
                        request: request,
                        id: localJobID,
                        owner: state.owner,
                        sessionEpoch: accountSessionID,
                        workload: workload
                    )
                case .submissionUncertain:
                    guard existingJob?.jobID == nil else {
                        throw MediaStudioVideoJobStoreError.jobConflict
                    }
                    _ = try await videoJobStore.stageSubmissionUncertain(
                        request: request,
                        id: localJobID,
                        owner: state.owner,
                        sessionEpoch: accountSessionID,
                        workload: workload
                    )
                default:
                    guard let providerJobID = existingJob?.jobID,
                          providerJobID == existingRecord.providerJobID else {
                        throw MediaStudioVideoJobStoreError.jobConflict
                    }
                    _ = try await videoJobStore.stageImportedProviderJob(
                        request: request,
                        id: localJobID,
                        providerJobID: providerJobID,
                        owner: state.owner,
                        sessionEpoch: accountSessionID,
                        workload: workload
                    )
                }
            } else if let existingJob {
                if let providerJobID = existingJob.jobID {
                    _ = try await videoJobStore.stageImportedProviderJob(
                        request: request,
                        id: localJobID,
                        providerJobID: providerJobID,
                        owner: state.owner,
                        sessionEpoch: accountSessionID,
                        workload: workload
                    )
                } else {
                    _ = try await videoJobStore.stageSubmissionUncertain(
                        request: request,
                        id: localJobID,
                        owner: state.owner,
                        sessionEpoch: accountSessionID,
                        workload: workload
                    )
                }
            } else {
                _ = try await videoJobStore.stagePrepared(
                    request: request,
                    id: localJobID,
                    owner: state.owner,
                    sessionEpoch: accountSessionID,
                    workload: workload
                )
                var intent = state
                intent.jobs[step.id] = .init(intentID: localJobID)
                intent.draft.segments[index].attempt = .init(
                    modelConfigID: model.id,
                    prompt: request.prompt,
                    size: request.size,
                    ratio: request.ratio,
                    seconds: request.seconds
                )
                intent.draft.segments[index].attempt?.id = localJobID
                intent.draft.segments[index].error = nil
                intent.events.append(.init(detail: "准备持久视频任务：\(project.segments[index].title)"))
                try await persist(intent)
            }

            try await check()
            let handedOff = try await store.handoffMediaBatchVideoGenerationToMediaJob(
                owner: state.owner,
                projectID: project.id,
                batchID: state.id,
                stepID: step.id,
                expectedIntentID: localJobID
            )
            state = handedOff
            await publish(handedOff)
            try await check()
            execution = try await videoJobStore.activate(
                id: localJobID,
                owner: state.owner,
                sessionEpoch: accountSessionID,
                workload: workload
            )
        } else {
            guard existingJob?.videoJobRecordID == localJobID else {
                throw StoryError.unresolvedSubmission
            }
            let snapshot = try await videoJobStore.snapshot(id: localJobID, owner: state.owner)
            guard snapshot.workload == workload else {
                throw MediaStudioVideoJobStoreError.jobConflict
            }
            if snapshot.isActive {
                guard let recovered = try await videoJobStore.recover(
                    owner: state.owner,
                    sessionEpoch: accountSessionID,
                    workload: workload
                ) else { throw StoryError.unresolvedSubmission }
                execution = recovered
            } else {
                execution = try await videoJobStore.activate(
                    id: localJobID,
                    owner: state.owner,
                    sessionEpoch: accountSessionID,
                    workload: workload
                )
            }
        }

        var phase = "reconciliation"
        var businessCommitted = false
        do {
            switch execution.mode {
            case .create:
                phase = "submitting"
                execution = try await videoJobStore.markSubmitting(
                    execution,
                    owner: state.owner,
                    sessionEpoch: accountSessionID
                )
                await publishVideoProgress(
                    state,
                    step.id,
                    .init(status: "submitting")
                )
            case let .resume(providerJobID):
                phase = "running"
                await publishVideoProgress(
                    state,
                    step.id,
                    .init(status: "checking", jobID: providerJobID)
                )
            case .needsReview:
                throw StoryError.unresolvedSubmission
            }

            progressError = nil
            persistedProgressStatus = execution.snapshot.status.rawValue
            persistedProviderJobID = execution.snapshot.providerJobID
            let callback: @Sendable (VideoGenerationProgress) async -> Void = { [self] value in
                await progress(value, stepID: step.id, localJobID: localJobID)
            }
            let result: VideoGenerationResult
            switch execution.mode {
            case .create:
                result = try await media.generateVideo(execution.request, progress: callback)
            case let .resume(providerJobID):
                guard let resumable = media as? any ResumableVideoGenerationServicing else {
                    throw StoryError.unavailable
                }
                result = try await resumable.resumeVideo(
                    execution.request,
                    jobID: providerJobID,
                    progress: callback
                )
            case .needsReview:
                throw StoryError.unresolvedSubmission
            }
            if let progressError { throw progressError }
            defer { result.videoFile?.removeIfNeeded() }

            try await check()
            phase = "finalizing"
            try await videoJobStore.markFinalizing(
                id: localJobID,
                owner: state.owner,
                sessionEpoch: accountSessionID,
                providerJobID: result.id
            )
            let video = try await store.saveVideo(
                result,
                projectID: project.id,
                owner: state.owner
            )
            var next = state
            guard next.jobs[step.id]?.videoJobRecordID == localJobID,
                  next.draft.segments[index].videoJobRecordID == localJobID,
                  next.draft.segments[index].attempt == nil else {
                throw StoryError.unresolvedSubmission
            }
            next.jobs[step.id]?.completed = true
            next.draft.segments[index].video = video
            next.draft.segments[index].error = nil
            next.events.append(.init(detail: "视频已保存：\(project.segments[index].title)"))
            try await persist(next)
            businessCommitted = true
            try await videoJobStore.markCompleted(
                id: localJobID,
                owner: state.owner,
                sessionEpoch: accountSessionID,
                providerJobID: result.id
            )
            try await applyActualLastFrameIfAvailable(video, step: step)
        } catch {
            if businessCommitted { throw error }
            if error is CancellationError || Task.isCancelled {
                try? await videoJobStore.markCancellation(
                    id: localJobID,
                    owner: state.owner,
                    sessionEpoch: accountSessionID
                )
                throw CancellationError()
            }
            let mayHaveSubmitted = (error as? any MediaGenerationSubmissionFailure)?
                .requestMayHaveBeenSubmitted ?? true
            do {
                try await videoJobStore.markFailure(
                    id: localJobID,
                    owner: state.owner,
                    sessionEpoch: accountSessionID,
                    phase: phase,
                    message: error.localizedDescription,
                    requestMayHaveBeenSubmitted: mayHaveSubmitted,
                    retryable: true
                )
            } catch MediaStudioVideoJobStoreError.staleSession {
                throw CancellationError()
            } catch {
                // The last durable state remains conservative and will be reconciled on resume.
            }
            throw error
        }
    }

    private func makeVideoRequest(
        project: StoryProject,
        segmentIndex index: Int,
        model: MediaGenerationModel,
        size: String,
        includesInputs: Bool
    ) async throws -> VideoGenerationRequest {
        let segment = project.segments[index]
        let prompt: String
        if let persistedPrompt = segment.attempt?.prompt {
            prompt = persistedPrompt
        } else {
            prompt = try StoryGenerationContext.videoPrompt(
                project,
                segment: segment,
                userIdeas: state.userIdeas ?? ""
            )
        }
        var image: ImageGenerationInputImage?
        var lastFrameImage: ImageGenerationInputImage?
        var referenceVideo: VideoGenerationInputVideo?
        var referencePurpose: VideoGenerationReferencePurpose = .reference
        if includesInputs {
            switch segment.videoGuidanceMode {
            case .firstFrame, .firstAndLastFrames:
                guard let frame = segment.firstFrame else { throw StoryError.invalidPlan }
                image = try videoInput(frame, name: "first-frame.png")
            case .previousVideo:
                guard model.supportsVideoReference, index > 0,
                      let previousVideo = project.segments[index - 1].video else {
                    throw StoryError.invalidPlan
                }
                referenceVideo = try await input(previousVideo, name: "previous-segment.mp4")
                referencePurpose = .extend
            case .sourceVideo:
                guard model.supportsVideoReference,
                      let sourceVideo = segment.archivedVideos.last else {
                    throw StoryError.invalidPlan
                }
                referenceVideo = try await input(sourceVideo, name: "source-video.mp4")
                referencePurpose = .edit
            }
            if model.supportsVideoLastFrame,
               segment.videoGuidanceMode == .firstAndLastFrames,
               let tail = segment.lastFrame {
                lastFrameImage = try videoInput(tail, name: "last-frame.png")
            }
        }
        return .init(
            modelConfigID: segment.attempt?.modelConfigID ?? model.id,
            prompt: prompt,
            size: segment.attempt?.size ?? size,
            seconds: segment.attempt?.seconds ?? segment.seconds,
            inputImage: image,
            lastFrameImage: lastFrameImage,
            referenceVideo: referenceVideo,
            referencePurpose: referencePurpose,
            ratio: segment.attempt?.ratio ?? project.ratio
        )
    }

    private func reconcileCompletedVideo(_ step: StoryMediaBatch.Step) async throws {
        guard let job = state.jobs[step.id],
              let localJobID = job.videoJobRecordID,
              let video = state.draft.segments.first(where: { $0.id == step.targetID })?.video else {
            return
        }
        let workload = MediaStudioVideoJobStore.Workload.storyBatchVideo(
            projectID: state.draft.id,
            batchID: state.id,
            segmentID: step.targetID,
            attemptID: localJobID
        )
        let snapshot = try await videoJobStore.snapshot(id: localJobID, owner: state.owner)
        guard snapshot.workload == workload, snapshot.providerJobID == video.jobID else {
            throw StoryError.unresolvedSubmission
        }
        _ = try await videoJobStore.activate(
            id: localJobID,
            owner: state.owner,
            sessionEpoch: accountSessionID,
            workload: workload
        )
        try await videoJobStore.markCompleted(
            id: localJobID,
            owner: state.owner,
            sessionEpoch: accountSessionID,
            providerJobID: video.jobID
        )
    }

    private func applyActualLastFrameIfAvailable(
        _ video: StoryVideo,
        step: StoryMediaBatch.Step
    ) async throws {
        do {
            let videoURL = try store.fileURL(
                video.filename,
                projectID: state.draft.id,
                owner: state.owner
            )
            let png = try await StoryVideoFrameExtractor.lastFramePNG(from: videoURL)
            let actualLastFrame = try await store.saveImage(
                png,
                mimeType: "image/png",
                projectID: state.draft.id,
                owner: state.owner,
                sourceResourceID: step.targetID,
                providerResultID: video.jobID,
                providerAssetID: "video-last-frame",
                derivedFromVideoJobID: video.jobID
            )
            guard let index = state.draft.segments.firstIndex(where: { $0.id == step.targetID }) else {
                throw StoryError.invalidPlan
            }
            var next = state
            next.draft.segments[index].lastFrames.images.append(actualLastFrame)
            next.draft.segments[index].actualVideoLastFrameID = actualLastFrame.id
            StoryContinuityContext.reconcileInheritedFirstFrames(&next.draft)
            try await persist(next)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            // Tail extraction is an enhancement; the generated video is already durable.
        }
    }

    private func imageInput(_ image: StoryImage, name: String) throws -> ImageGenerationInputImage {
        let url = try store.fileURL(image.filename, projectID: state.draft.id, owner: state.owner)
        return .init(name: name, mimeType: image.mimeType, fileURL: url)
    }
    private func videoInput(_ image: StoryImage, name: String) throws -> ImageGenerationInputImage {
        let url = try store.fileURL(
            image.filename,
            projectID: state.draft.id,
            owner: state.owner
        )
        return .init(name: name, mimeType: image.mimeType, fileURL: url)
    }
    private func input(_ video: StoryVideo, name: String) async throws -> VideoGenerationInputVideo {
        let url = try store.fileURL(video.filename, projectID: state.draft.id, owner: state.owner)
        return .init(name: name, mimeType: "video/mp4", fileURL: url)
    }
    private func progress(
        _ value: VideoGenerationProgress,
        stepID: String,
        localJobID: UUID
    ) async {
        do {
            try await check()
            guard progressError == nil else { return }
            let changesJobID = value.jobID.map { $0 != persistedProviderJobID } ?? false
            let changesStatus = value.status != persistedProgressStatus
            guard changesJobID || changesStatus else { return }
            try await videoJobStore.recordProgress(
                id: localJobID,
                owner: state.owner,
                sessionEpoch: accountSessionID,
                progress: value
            )
            persistedProgressStatus = value.status
            if let providerJobID = value.jobID { persistedProviderJobID = providerJobID }
            await publishVideoProgress(state, stepID, value)
        } catch {
            progressError = error
            withUnsafeCurrentTask { $0?.cancel() }
        }
    }

    private func persist(_ input: StoryMediaBatch) async throws {
        try await check()
        try await store.commitMediaBatch(state)
        var next = input; next.expectedProjectDigest = try StoryAgentRun.digest(state.draft); next.updatedAt = Date()
        state = next // Keep recovery information even if the next atomic write fails.
        try await store.commitMediaBatch(next)
        await publish(next)
    }
}
