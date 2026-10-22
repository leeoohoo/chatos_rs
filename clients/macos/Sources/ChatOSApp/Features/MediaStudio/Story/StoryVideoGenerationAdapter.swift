import ChatOSAgentRuntime
import ChatOSCore
import Foundation

private actor StoryVideoProgressPersistenceGate {
    private var persistedStatus: String?
    private var persistedJobID: String?
    private(set) var error: (any Error)?

    func persist(
        for progress: VideoGenerationProgress,
        operation: @Sendable () async throws -> Void
    ) async throws {
        let introducesJobID = progress.jobID.map { $0 != persistedJobID } ?? false
        guard progress.status != persistedStatus || introducesJobID else { return }
        try await operation()
        persistedStatus = progress.status
        if let jobID = progress.jobID { persistedJobID = jobID }
    }

    func fail(_ error: any Error) {
        if self.error == nil { self.error = error }
    }
}

/// macOS adapter for one Story video attempt. Provider requests/results, job identity,
/// local media bytes, filenames, Store intents and tail-frame extraction remain private.
struct StoryVideoGenerationAdapter: StoryVideoGenerating {
    let store: StoryProjectStore
    let media: any MediaGenerationServicing
    let videoJobStore: MediaStudioVideoJobStore

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

    func releaseStoryVideoGenerationAfterVerification(
        ownerUserID: String,
        accountSessionID: UUID,
        project: StoryProject,
        segmentID: String,
        continuationCheck: @escaping @Sendable () async throws -> Void
    ) async throws -> StoryProject {
        guard let segment = project.segments.first(where: { $0.id == segmentID }),
              segment.video == nil,
              segment.attempt == nil,
              let localJobID = segment.videoJobRecordID else {
            throw StoryError.invalidPlan
        }
        let workload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: project.id,
            segmentID: segmentID,
            attemptID: localJobID
        )
        try await continuationCheck()
        let initial = try await videoJobStore.snapshot(id: localJobID, owner: ownerUserID)
        guard initial.workload == workload else {
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
        // The terminal marker must win the crash order. A session change here intentionally
        // leaves the Story association for the next verified, idempotent replay to finish.
        try await continuationCheck()
        return try await store.releaseVideoJobAfterVerification(
            projectID: project.id,
            segmentID: segmentID,
            jobRecordID: localJobID,
            owner: ownerUserID
        )
    }

    func executeStoryVideoGeneration(
        ownerUserID: String,
        accountSessionID: UUID,
        command: StoryVideoGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryVideoGenerationEvent) async -> Void
    ) async throws -> StoryVideoGenerationOutcome {
        let localJobID = command.attempt.id
        let workload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: command.projectID,
            segmentID: command.segmentID,
            attemptID: localJobID
        )
        var durableProject: StoryProject?
        var activated = false
        var phase = "preparing"
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
            switch command.kind {
            case let .start(model):
                let intent = try await store.beginVideoGeneration(
                    projectID: command.projectID,
                    segmentID: command.segmentID,
                    attempt: command.attempt,
                    owner: ownerUserID
                )
                try await continuationCheck()
                request = try await makeRequest(
                    command: command,
                    intent: intent,
                    model: model,
                    ownerUserID: ownerUserID,
                    continuationCheck: continuationCheck
                )
                phase = "staging"
                _ = try await videoJobStore.stagePrepared(
                    request: request,
                    id: localJobID,
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID,
                    workload: workload
                )
            case let .resume(jobID):
                request = .init(
                    modelConfigID: command.attempt.modelConfigID,
                    prompt: command.attempt.prompt,
                    size: command.attempt.size,
                    seconds: command.attempt.seconds,
                    ratio: command.attempt.ratio
                )
                phase = "importing"
                _ = try await videoJobStore.stageImportedProviderJob(
                    request: request,
                    id: localJobID,
                    providerJobID: jobID,
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID,
                    workload: workload
                )
            }

            try await continuationCheck()
            phase = "handoff"
            let handedOff = try await store.handoffVideoGenerationToMediaJob(
                projectID: command.projectID,
                segmentID: command.segmentID,
                expectedAttempt: command.attempt,
                owner: ownerUserID
            )
            durableProject = handedOff
            await onEvent(.init(
                project: handedOff,
                progress: .init(status: "prepared")
            ))

            try await continuationCheck()
            phase = "activating"
            var execution = try await videoJobStore.activate(
                id: localJobID,
                owner: ownerUserID,
                sessionEpoch: accountSessionID,
                workload: workload
            )
            activated = true

            switch execution.mode {
            case .create:
                phase = "submitting"
                execution = try await videoJobStore.markSubmitting(
                    id: localJobID,
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID
                )
                await onEvent(.init(
                    project: handedOff,
                    progress: .init(status: "submitting")
                ))
            case let .resume(providerJobID):
                phase = "running"
                await onEvent(.init(
                    project: handedOff,
                    progress: .init(status: "checking")
                ))
                guard case .resume(let expectedProviderJobID) = command.kind,
                      expectedProviderJobID == providerJobID else {
                    throw StoryError.unresolvedSubmission
                }
            case .needsReview:
                throw StoryError.unresolvedSubmission
            }

            let persistenceGate = StoryVideoProgressPersistenceGate()
            let progress = progressHandler(
                localJobID: localJobID,
                ownerUserID: ownerUserID,
                accountSessionID: accountSessionID,
                project: handedOff,
                persistenceGate: persistenceGate,
                onEvent: onEvent
            )
            let result: VideoGenerationResult
            do {
                switch execution.mode {
                case .create:
                    result = try await service.generateVideo(execution.request, progress: progress)
                case let .resume(providerJobID):
                    guard let resumable = service as? any ResumableVideoGenerationServicing else {
                        throw StoryError.unavailable
                    }
                    result = try await resumable.resumeVideo(
                        execution.request,
                        jobID: providerJobID,
                        progress: progress
                    )
                case .needsReview:
                    throw StoryError.unresolvedSubmission
                }
            } catch {
                if let persistenceError = await persistenceGate.error { throw persistenceError }
                throw error
            }
            if let persistenceError = await persistenceGate.error { throw persistenceError }
            defer { result.videoFile?.removeIfNeeded() }

            try await continuationCheck()
            phase = "finalizing"
            try await videoJobStore.markFinalizing(
                id: localJobID,
                owner: ownerUserID,
                sessionEpoch: accountSessionID,
                providerJobID: result.id
            )
            await onEvent(.init(
                project: handedOff,
                progress: .init(status: "saving")
            ))
            let completed = try await store.completeVideoGenerationFromMediaJob(
                result,
                projectID: command.projectID,
                segmentID: command.segmentID,
                jobRecordID: localJobID,
                owner: ownerUserID
            )
            do {
                try await videoJobStore.markCompleted(
                    id: localJobID,
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID,
                    providerJobID: result.id
                )
            } catch {
                // The Story result is already durable and replay-safe. Leave the job in
                // finalizing so startup recovery can reconcile it without duplicating media.
                await onEvent(.init(
                    project: completed.0,
                    progress: .init(status: "saving"),
                    error: "视频已保存，任务状态将在下次恢复时核对：\(error.localizedDescription)"
                ))
                return .init(project: completed.0, status: .completed)
            }
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
            if error is CancellationError || Task.isCancelled {
                if activated {
                    try? await videoJobStore.markCancellation(
                        id: localJobID,
                        owner: ownerUserID,
                        sessionEpoch: accountSessionID
                    )
                }
                throw CancellationError()
            }
            do {
                try await continuationCheck()
            } catch {
                if activated {
                    try? await videoJobStore.markCancellation(
                        id: localJobID,
                        owner: ownerUserID,
                        sessionEpoch: accountSessionID
                    )
                }
                throw CancellationError()
            }
            let failed: StoryProject
            let status: StoryVideoGenerationOutcomeStatus
            if durableProject != nil {
                if activated {
                    let mayHaveSubmitted = (error as? any MediaGenerationSubmissionFailure)?
                        .requestMayHaveBeenSubmitted ?? true
                    do {
                        try await videoJobStore.markFailure(
                            id: localJobID,
                            owner: ownerUserID,
                            sessionEpoch: accountSessionID,
                            phase: phase,
                            message: error.localizedDescription,
                            requestMayHaveBeenSubmitted: mayHaveSubmitted,
                            retryable: true
                        )
                    } catch MediaStudioVideoJobStoreError.staleSession {
                        throw CancellationError()
                    } catch {
                        // The Story error is still useful when a same-session state write
                        // fails; the durable manifest remains conservative for recovery.
                    }
                }
                failed = try await store.failVideoGenerationFromMediaJob(
                    error.localizedDescription,
                    projectID: command.projectID,
                    segmentID: command.segmentID,
                    jobRecordID: localJobID,
                    owner: ownerUserID
                )
                status = .failedRetainingAttempt
            } else if case .start = command.kind {
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

    func recoverStoryVideoGeneration(
        ownerUserID: String,
        accountSessionID: UUID,
        project: StoryProject,
        segmentID: String,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryVideoGenerationEvent) async -> Void
    ) async throws -> StoryVideoGenerationRecoveryOutcome {
        guard let segment = project.segments.first(where: { $0.id == segmentID }),
              let localJobID = segment.videoJobRecordID,
              segment.attempt == nil else { throw StoryError.invalidPlan }
        let workload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: project.id,
            segmentID: segmentID,
            attemptID: localJobID
        )
        var activated = false
        var phase = "recovery"
        do {
            try await continuationCheck()
            let service: any MediaGenerationServicing
            if let bindable = media as? any SessionBoundMediaGenerationServicing {
                service = try await bindable.boundToCurrentSession()
            } else {
                service = media
            }
            try await continuationCheck()

            let initial = try await videoJobStore.snapshot(id: localJobID, owner: ownerUserID)
            guard initial.workload == workload else {
                throw MediaStudioVideoJobStoreError.jobConflict
            }

            // Story is committed before the job terminal marker. Reconcile that crash window
            // locally without querying or downloading the provider result a second time.
            if let video = segment.video {
                guard initial.providerJobID == video.jobID else {
                    return try await needsReview(
                        project: project,
                        segmentID: segmentID,
                        localJobID: localJobID,
                        ownerUserID: ownerUserID,
                        message: "剧情视频已保存，但服务商任务 ID 与持久任务记录不一致，请人工核对。"
                    )
                }
                if initial.status == .completed {
                    return .finished(.init(project: project, status: .completed))
                }
                let claimed = try await videoJobStore.activate(
                    id: localJobID,
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID,
                    workload: workload
                )
                activated = true
                if claimed.snapshot.status != .completed {
                    try await videoJobStore.markCompleted(
                        id: localJobID,
                        owner: ownerUserID,
                        sessionEpoch: accountSessionID,
                        providerJobID: video.jobID
                    )
                }
                return .finished(.init(project: project, status: .completed))
            }

            let execution: MediaStudioVideoJobStore.Execution
            if initial.isActive {
                guard let recovered = try await videoJobStore.recover(
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID,
                    workload: workload
                ) else {
                    let message: String
                    if initial.status == .completed {
                        message = "持久任务已完成，但剧情视频文件尚未提交，请人工核对。"
                    } else {
                        message = initial.failureMessage
                            ?? "视频任务当前不能自动恢复，请核对服务商任务记录。"
                    }
                    return try await needsReview(
                        project: project,
                        segmentID: segmentID,
                        localJobID: localJobID,
                        ownerUserID: ownerUserID,
                        message: message
                    )
                }
                execution = recovered
            } else {
                execution = try await videoJobStore.activate(
                    id: localJobID,
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID,
                    workload: workload
                )
            }
            activated = true

            switch execution.mode {
            case .needsReview:
                return try await needsReview(
                    project: project,
                    segmentID: segmentID,
                    localJobID: localJobID,
                    ownerUserID: ownerUserID,
                    message: execution.snapshot.failureMessage
                        ?? "上次视频提交结果尚未确认。为避免重复计费，客户端不会自动重发。"
                )
            case .create, .resume:
                break
            }

            var running = execution
            switch execution.mode {
            case .create:
                phase = "submitting"
                running = try await videoJobStore.markSubmitting(
                    id: localJobID,
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID
                )
                await onEvent(.init(
                    project: project,
                    progress: .init(status: "submitting")
                ))
            case let .resume(providerJobID):
                phase = "running"
                await onEvent(.init(
                    project: project,
                    progress: .init(status: "checking")
                ))
                guard running.snapshot.providerJobID == providerJobID else {
                    throw StoryError.unresolvedSubmission
                }
            case .needsReview:
                throw StoryError.unresolvedSubmission
            }

            let persistenceGate = StoryVideoProgressPersistenceGate()
            let progress = progressHandler(
                localJobID: localJobID,
                ownerUserID: ownerUserID,
                accountSessionID: accountSessionID,
                project: project,
                persistenceGate: persistenceGate,
                onEvent: onEvent
            )
            let result: VideoGenerationResult
            do {
                switch running.mode {
                case .create:
                    result = try await service.generateVideo(running.request, progress: progress)
                case let .resume(providerJobID):
                    guard let resumable = service as? any ResumableVideoGenerationServicing else {
                        throw StoryError.unavailable
                    }
                    result = try await resumable.resumeVideo(
                        running.request,
                        jobID: providerJobID,
                        progress: progress
                    )
                case .needsReview:
                    throw StoryError.unresolvedSubmission
                }
            } catch {
                if let persistenceError = await persistenceGate.error { throw persistenceError }
                throw error
            }
            if let persistenceError = await persistenceGate.error { throw persistenceError }
            defer { result.videoFile?.removeIfNeeded() }

            try await continuationCheck()
            phase = "finalizing"
            try await videoJobStore.markFinalizing(
                id: localJobID,
                owner: ownerUserID,
                sessionEpoch: accountSessionID,
                providerJobID: result.id
            )
            await onEvent(.init(project: project, progress: .init(status: "saving")))
            let completed = try await store.completeVideoGenerationFromMediaJob(
                result,
                projectID: project.id,
                segmentID: segmentID,
                jobRecordID: localJobID,
                owner: ownerUserID
            )
            do {
                try await videoJobStore.markCompleted(
                    id: localJobID,
                    owner: ownerUserID,
                    sessionEpoch: accountSessionID,
                    providerJobID: result.id
                )
            } catch {
                await onEvent(.init(
                    project: completed.0,
                    progress: .init(status: "saving"),
                    error: "视频已保存，任务状态将在下次恢复时核对：\(error.localizedDescription)"
                ))
                return .finished(.init(project: completed.0, status: .completed))
            }
            result.videoFile?.removeIfNeeded()
            var finalProject = completed.0
            do {
                let videoURL = try store.fileURL(
                    completed.1.filename,
                    projectID: project.id,
                    owner: ownerUserID
                )
                let finalFrame = try await StoryVideoFrameExtractor.lastFramePNG(from: videoURL)
                try await continuationCheck()
                finalProject = try await store.applyActualVideoLastFrame(
                    finalFrame,
                    projectID: project.id,
                    segmentID: segmentID,
                    videoJobID: completed.1.jobID,
                    owner: ownerUserID
                ).0
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                // The provider video is already durable. Tail extraction is a local enhancement.
            }
            try await continuationCheck()
            return .finished(.init(project: finalProject, status: .completed))
        } catch {
            if error is CancellationError || Task.isCancelled {
                if activated {
                    try? await videoJobStore.markCancellation(
                        id: localJobID,
                        owner: ownerUserID,
                        sessionEpoch: accountSessionID
                    )
                }
                throw CancellationError()
            }
            do {
                try await continuationCheck()
            } catch {
                throw CancellationError()
            }
            if activated {
                let mayHaveSubmitted = (error as? any MediaGenerationSubmissionFailure)?
                    .requestMayHaveBeenSubmitted ?? true
                do {
                    try await videoJobStore.markFailure(
                        id: localJobID,
                        owner: ownerUserID,
                        sessionEpoch: accountSessionID,
                        phase: phase,
                        message: error.localizedDescription,
                        requestMayHaveBeenSubmitted: mayHaveSubmitted,
                        retryable: true
                    )
                } catch MediaStudioVideoJobStoreError.staleSession {
                    throw CancellationError()
                } catch {
                    // Preserve the original provider/recovery error for the Story surface.
                }
            }
            let failed = try await store.failVideoGenerationFromMediaJob(
                error.localizedDescription,
                projectID: project.id,
                segmentID: segmentID,
                jobRecordID: localJobID,
                owner: ownerUserID
            )
            return .finished(.init(project: failed, status: .failedRetainingAttempt))
        }
    }

    private func needsReview(
        project: StoryProject,
        segmentID: String,
        localJobID: UUID,
        ownerUserID: String,
        message: String
    ) async throws -> StoryVideoGenerationRecoveryOutcome {
        let reviewed = try await store.failVideoGenerationFromMediaJob(
            message,
            projectID: project.id,
            segmentID: segmentID,
            jobRecordID: localJobID,
            owner: ownerUserID
        )
        return .needsReview(project: reviewed, message: message)
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
        localJobID: UUID,
        ownerUserID: String,
        accountSessionID: UUID,
        project: StoryProject,
        persistenceGate: StoryVideoProgressPersistenceGate,
        onEvent: @escaping @Sendable (StoryVideoGenerationEvent) async -> Void
    ) -> @Sendable (VideoGenerationProgress) async -> Void {
        return { value in
            do {
                try await persistenceGate.persist(for: value) {
                    try await videoJobStore.recordProgress(
                        id: localJobID,
                        owner: ownerUserID,
                        sessionEpoch: accountSessionID,
                        progress: value
                    )
                }
                await onEvent(.init(
                    project: project,
                    progress: .init(status: value.status, percent: value.percent)
                ))
            } catch {
                await persistenceGate.fail(error)
                await onEvent(.init(
                    project: project,
                    progress: .init(status: value.status, percent: value.percent),
                    error: "保存任务 ID 失败，请保留任务 ID \(value.jobID ?? "未知")，勿重复提交：\(error.localizedDescription)"
                ))
                withUnsafeCurrentTask { $0?.cancel() }
            }
        }
    }
}
