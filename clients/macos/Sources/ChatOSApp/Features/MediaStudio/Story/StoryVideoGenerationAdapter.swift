import ChatOSAgentRuntime
import ChatOSCore
import Foundation

private actor StoryVideoProgressPersistenceGate {
    private var project: StoryProject
    private var persistedStatus: String?
    private var persistedJobID: String?

    init(project: StoryProject, segmentID: String) {
        self.project = project
        let attempt = project.segments.first(where: { $0.id == segmentID })?.attempt
        persistedStatus = attempt?.status
        persistedJobID = attempt?.jobID
    }

    func project(
        for progress: VideoGenerationProgress,
        persist: @Sendable () async throws -> StoryProject
    ) async throws -> StoryProject {
        let introducesJobID = progress.jobID.map { $0 != persistedJobID } ?? false
        guard progress.status != persistedStatus || introducesJobID else { return project }
        let saved = try await persist()
        project = saved
        persistedStatus = progress.status
        if let jobID = progress.jobID { persistedJobID = jobID }
        return saved
    }
}

/// macOS adapter for one Story video attempt. Provider requests/results, job identity,
/// local media bytes, filenames, Store intents and tail-frame extraction remain private.
struct StoryVideoGenerationAdapter: StoryVideoGenerating {
    let store: StoryProjectStore
    let media: any MediaGenerationServicing

    nonisolated func prepareStoryVideoGeneration(
        ownerUserID: String,
        project: StoryProject,
        segmentID: String,
        model: MediaGenerationModel,
        size: String,
        userIdeas: String
    ) throws -> StoryVideoGenerationCommand {
        guard let segment = project.segments.first(where: { $0.id == segmentID }) else {
            throw StoryError.invalidPlan
        }
        let prompt = try StoryGenerationContext.videoPrompt(
            project,
            segment: segment,
            userIdeas: userIdeas
        )
        return .init(
            project: project,
            segmentID: segmentID,
            attempt: .init(
                modelConfigID: project.models.videoModelID,
                prompt: prompt,
                size: size,
                ratio: project.ratio,
                seconds: segment.seconds
            ),
            kind: .start(model: model)
        )
    }

    func executeStoryVideoGeneration(
        ownerUserID: String,
        command: StoryVideoGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryVideoGenerationEvent) async -> Void
    ) async throws -> StoryVideoGenerationOutcome {
        do {
            try await continuationCheck()
            let service: any MediaGenerationServicing
            if let bindable = media as? any SessionBoundMediaGenerationServicing {
                service = try await bindable.boundToCurrentSession()
            } else {
                service = media
            }
            try await continuationCheck()

            let request: VideoGenerationRequest
            let result: VideoGenerationResult
            switch command.kind {
            case let .start(model):
                let intent = try await store.beginVideoGeneration(
                    projectID: command.projectID,
                    segmentID: command.segmentID,
                    attempt: command.attempt,
                    owner: ownerUserID
                )
                await onEvent(.init(
                    project: intent.project,
                    progress: .init(status: "submitting")
                ))
                try await continuationCheck()
                request = try await makeRequest(
                    command: command,
                    intent: intent,
                    model: model,
                    ownerUserID: ownerUserID,
                    continuationCheck: continuationCheck
                )
                result = try await service.generateVideo(
                    request,
                    progress: progressHandler(
                        command: command,
                        ownerUserID: ownerUserID,
                        fallbackProject: intent.project,
                        onEvent: onEvent
                    )
                )
            case let .resume(jobID):
                request = .init(
                    modelConfigID: command.attempt.modelConfigID,
                    prompt: command.attempt.prompt,
                    size: command.attempt.size,
                    seconds: command.attempt.seconds,
                    ratio: command.attempt.ratio
                )
                await onEvent(.init(
                    project: command.project,
                    progress: .init(status: "checking")
                ))
                guard let resumable = service as? any ResumableVideoGenerationServicing else {
                    throw StoryError.unavailable
                }
                result = try await resumable.resumeVideo(
                    request,
                    jobID: jobID,
                    progress: progressHandler(
                        command: command,
                        ownerUserID: ownerUserID,
                        fallbackProject: command.project,
                        onEvent: onEvent
                    )
                )
            }

            try await continuationCheck()
            let completed = try await store.completeVideoGeneration(
                result,
                projectID: command.projectID,
                segmentID: command.segmentID,
                attemptID: command.attempt.id,
                owner: ownerUserID
            )
            result.videoFile?.removeIfNeeded()
            var finalProject = completed.0
            do {
                let videoURL = try store.fileURL(
                    completed.1.filename,
                    projectID: command.projectID,
                    owner: ownerUserID
                )
                let finalFrame = try await StoryVideoFrameExtractor.lastFramePNG(from: videoURL)
                try await continuationCheck()
                finalProject = try await store.applyActualVideoLastFrame(
                    finalFrame,
                    projectID: command.projectID,
                    segmentID: command.segmentID,
                    videoJobID: completed.1.jobID,
                    owner: ownerUserID
                ).0
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                // The provider video is already durable. Tail extraction is a local enhancement.
            }
            try await continuationCheck()
            return .init(project: finalProject, status: .completed)
        } catch {
            if error is CancellationError || Task.isCancelled { throw CancellationError() }
            let failed: StoryProject
            let status: StoryVideoGenerationOutcomeStatus
            if let failure = error as? any MediaGenerationSubmissionFailure,
               !failure.requestMayHaveBeenSubmitted {
                failed = try await store.failVideoGenerationBeforeSubmission(
                    error.localizedDescription,
                    projectID: command.projectID,
                    segmentID: command.segmentID,
                    attemptID: command.attempt.id,
                    owner: ownerUserID
                )
                status = .failedBeforeSubmission
            } else {
                failed = try await store.failVideoGeneration(
                    error.localizedDescription,
                    projectID: command.projectID,
                    segmentID: command.segmentID,
                    attemptID: command.attempt.id,
                    owner: ownerUserID
                )
                status = .failedRetainingAttempt
            }
            return .init(project: failed, status: status)
        }
    }

    private func makeRequest(
        command: StoryVideoGenerationCommand,
        intent: StoryProjectStore.VideoGenerationIntent,
        model: MediaGenerationModel,
        ownerUserID: String,
        continuationCheck: @escaping @Sendable () async throws -> Void
    ) async throws -> VideoGenerationRequest {
        var firstFrameInput: ImageGenerationInputImage?
        var lastFrameInput: ImageGenerationInputImage?
        var referenceVideoInput: VideoGenerationInputVideo?
        var referencePurpose: VideoGenerationReferencePurpose = .reference
        switch intent.segment.videoGuidanceMode {
        case .firstFrame, .firstAndLastFrames:
            guard let frame = intent.segment.firstFrame else { throw StoryError.invalidPlan }
            let url = try store.fileURL(
                frame.filename,
                projectID: command.projectID,
                owner: ownerUserID
            )
            try await continuationCheck()
            firstFrameInput = .init(
                name: "first-frame.png",
                mimeType: frame.mimeType,
                fileURL: url
            )
        case .previousVideo, .sourceVideo:
            guard model.supportsVideoReference,
                  let index = intent.project.segments.firstIndex(where: {
                    $0.id == command.segmentID
                  }) else { throw StoryError.invalidPlan }
            let referenceVideo = intent.segment.videoGuidanceMode == .previousVideo
                ? (index > 0 ? intent.project.segments[index - 1].video : nil)
                : intent.segment.archivedVideos.last
            guard let referenceVideo else { throw StoryError.invalidPlan }
            let url = try store.fileURL(
                referenceVideo.filename,
                projectID: command.projectID,
                owner: ownerUserID
            )
            try await continuationCheck()
            referenceVideoInput = .init(
                name: intent.segment.videoGuidanceMode == .previousVideo
                    ? "previous-segment.mp4" : "source-video.mp4",
                mimeType: "video/mp4",
                fileURL: url
            )
            referencePurpose = intent.segment.videoGuidanceMode == .previousVideo
                ? .extend : .edit
        }
        if model.supportsVideoLastFrame,
           intent.segment.videoGuidanceMode == .firstAndLastFrames,
           let lastFrame = intent.segment.lastFrame {
            let url = try store.fileURL(
                lastFrame.filename,
                projectID: command.projectID,
                owner: ownerUserID
            )
            try await continuationCheck()
            lastFrameInput = .init(
                name: "last-frame.png",
                mimeType: lastFrame.mimeType,
                fileURL: url
            )
        }
        return .init(
            modelConfigID: command.attempt.modelConfigID,
            prompt: command.attempt.prompt,
            size: command.attempt.size,
            seconds: command.attempt.seconds,
            inputImage: firstFrameInput,
            lastFrameImage: lastFrameInput,
            referenceVideo: referenceVideoInput,
            referencePurpose: referencePurpose,
            ratio: command.attempt.ratio
        )
    }

    private func progressHandler(
        command: StoryVideoGenerationCommand,
        ownerUserID: String,
        fallbackProject: StoryProject,
        onEvent: @escaping @Sendable (StoryVideoGenerationEvent) async -> Void
    ) -> @Sendable (VideoGenerationProgress) async -> Void {
        let persistenceGate = StoryVideoProgressPersistenceGate(
            project: fallbackProject,
            segmentID: command.segmentID
        )
        return { value in
            do {
                let project = try await persistenceGate.project(for: value) {
                    try await store.updateVideoGenerationProgress(
                        value,
                        projectID: command.projectID,
                        segmentID: command.segmentID,
                        attemptID: command.attempt.id,
                        owner: ownerUserID
                    )
                }
                await onEvent(.init(
                    project: project,
                    progress: .init(status: value.status, percent: value.percent)
                ))
            } catch {
                await onEvent(.init(
                    project: fallbackProject,
                    progress: .init(status: value.status, percent: value.percent),
                    error: "保存任务 ID 失败，请保留任务 ID \(value.jobID ?? "未知")，勿重复提交：\(error.localizedDescription)"
                ))
                withUnsafeCurrentTask { $0?.cancel() }
            }
        }
    }
}
