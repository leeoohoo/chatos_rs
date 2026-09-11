import AppKit
import ChatOSCore
import Foundation

/// A serial, deterministic queue. Waiting for a video never calls a text model.
actor StoryMediaBatchSession {
    private var state: StoryMediaBatch
    private let store: StoryProjectStore
    private let media: any MediaGenerationServicing
    private let check: @Sendable () async throws -> Void
    private let shouldPause: @Sendable () async -> Bool
    private let publish: @Sendable (StoryMediaBatch) async -> Void
    private var progressError: (any Error)?

    init(batch: StoryMediaBatch, store: StoryProjectStore, media: any MediaGenerationServicing,
         check: @escaping @Sendable () async throws -> Void = { try Task.checkCancellation() },
         shouldPause: @escaping @Sendable () async -> Bool = { false },
         publish: @escaping @Sendable (StoryMediaBatch) async -> Void = { _ in }) {
        self.state = batch; self.store = store; self.media = media; self.check = check
        self.shouldPause = shouldPause; self.publish = publish
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
                if state.jobs[step.id]?.completed == true { continue }
                if await shouldPause() {
                    var paused = state; paused.status = .paused; try await persist(paused); return state
                }
                if let job = state.jobs[step.id], !(step.kind == .videos && job.jobID != nil) { throw StoryError.unresolvedSubmission }
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
            if !Task.isCancelled { try await persist(stopped) }
            else { throw error }
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
            next.events.append(.init(detail: "已直接承接上一段尾帧作为首帧：\(id)，未调用图片模型"))
            try await persist(next)
            return
        }
        if step.kind == .videos {
            guard let index = project.segments.firstIndex(where: { $0.id == id }), project.segments[index].detail != nil,
                  let model = state.models.first(where: { $0.id == project.models.videoModelID }) else { throw StoryError.invalidPlan }
            let oldJob = state.jobs[key]
            let profile = VideoGenerationProfile(modelName: model.modelName)
            guard profile.durations.contains(project.segments[index].seconds) else {
                throw StoryError.unsupportedDuration
            }
            let prompt = try StoryGenerationContext.videoPrompt(project, segment: project.segments[index])
            var image: ImageGenerationInputImage?
            var lastFrameImage: ImageGenerationInputImage?
            if oldJob == nil {
                guard let frame = project.segments[index].firstFrame else { throw StoryError.invalidPlan }
                image = try await input(frame, name: "first-frame.png")
                if model.supportsVideoLastFrame, project.segments[index].useLastFrameForVideo,
                   let tail = project.segments[index].lastFrame {
                    lastFrameImage = try await input(tail, name: "last-frame.png")
                }
            }
            let request = VideoGenerationRequest(modelConfigID: model.id, prompt: prompt, size: profile.sizes[0],
                                                 seconds: project.segments[index].seconds,
                                                 inputImage: image, lastFrameImage: lastFrameImage, ratio: project.ratio)
            if oldJob == nil {
                var next = state; next.jobs[key] = .init()
                next.draft.segments[index].attempt = .init(modelConfigID: model.id, prompt: prompt, size: request.size,
                                                           ratio: request.ratio, seconds: request.seconds)
                next.draft.segments[index].error = nil
                next.events.append(.init(detail: "提交视频：\(project.segments[index].title)"))
                try await persist(next)
            }
            try await check(); progressError = nil
            let callback: @Sendable (VideoGenerationProgress) async -> Void = { [self] value in await progress(value, step: step) }
            let result: VideoGenerationResult
            if let jobID = oldJob?.jobID {
                guard let resumable = media as? any ResumableVideoGenerationServicing else { throw StoryError.unavailable }
                result = try await resumable.resumeVideo(request, jobID: jobID, progress: callback)
            } else { result = try await media.generateVideo(request, progress: callback) }
            if let progressError { throw progressError }
            try await check()
            let video = try await store.saveVideo(result, projectID: project.id, owner: state.owner)
            var next = state; next.jobs[key]?.completed = true; next.jobs[key]?.jobID = result.id
            next.draft.segments[index].video = video
            next.draft.segments[index].attempt?.jobID = result.id; next.draft.segments[index].attempt?.status = "completed"
            next.draft.segments[index].error = nil
            next.events.append(.init(detail: "视频已保存：\(project.segments[index].title)"))
            try await persist(next)
        } else {
            var request: ImageGenerationRequest
            if step.kind == .assets {
                guard let asset = project.resource(id: id) else { throw StoryError.invalidPlan }
                request = .init(modelConfigID: project.models.imageModelID,
                                prompt: StoryGenerationContext.assetPrompt(project, resource: asset), size: nil, count: 1)
            } else {
                guard let segment = project.segments.first(where: { $0.id == id }), segment.detail != nil else { throw StoryError.invalidPlan }
                var references: [ImageGenerationInputImage] = []
                for assetID in segment.resourceIDs {
                    guard let asset = project.resource(id: assetID), let image = asset.confirmedImage else { throw StoryError.invalidPlan }
                    references.append(try await input(image, name: asset.name + ".png"))
                }
                var previousTailReferenceIndex: Int?
                var currentFirstFrameReferenceIndex: Int?
                if step.kind == .frames,
                   let previous = StoryContinuityContext.previousTail(project, segmentID: segment.id) {
                    references.append(try await input(previous.image, name: "previous-segment-last-frame.png"))
                    previousTailReferenceIndex = references.count
                } else if step.kind == .lastFrames, let firstFrame = segment.firstFrame {
                    references.append(try await input(firstFrame, name: "current-segment-first-frame.png"))
                    currentFirstFrameReferenceIndex = references.count
                }
                request = .init(modelConfigID: project.models.imageModelID,
                    prompt: try StoryGenerationContext.framePrompt(project, segment: segment,
                                                                   role: step.kind == .frames ? .first : .last,
                                                                   referenceResourceIDs: segment.resourceIDs,
                                                                   previousTailReferenceIndex: previousTailReferenceIndex,
                                                                   currentFirstFrameReferenceIndex: currentFirstFrameReferenceIndex),
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
            let bytes = try await MediaStudioImageLoader.data(for: image)
            guard let bitmap = NSBitmapImageRep(data: bytes), let png = bitmap.representation(using: .png, properties: [:]) else { throw StoryError.unsafeFile }
            let stored = try await store.saveImage(png, mimeType: "image/png", projectID: project.id, owner: state.owner,
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
    }

    private func input(_ image: StoryImage, name: String) async throws -> ImageGenerationInputImage {
        let url = try store.fileURL(image.filename, projectID: state.draft.id, owner: state.owner)
        let bytes = try await MediaStudioImageLoader.data(for: .init(id: image.id.uuidString, mimeType: image.mimeType, url: url))
        return .init(name: name, mimeType: image.mimeType, base64Data: bytes.base64EncodedString())
    }
    private func progress(_ value: VideoGenerationProgress, step: StoryMediaBatch.Step) async {
        do {
            try await check()
            guard progressError == nil, let index = state.draft.segments.firstIndex(where: { $0.id == step.targetID }) else { return }
            var next = state
            if let id = value.jobID {
                guard next.jobs[step.id]?.jobID == nil || next.jobs[step.id]?.jobID == id else { throw StoryAgentError.invalidRun }
                next.jobs[step.id]?.jobID = id; next.draft.segments[index].attempt?.jobID = id
            }
            next.draft.segments[index].attempt?.status = value.status
            next.events.append(.init(detail: "\(step.targetID)：\(value.status)，任务 ID：\(value.jobID ?? next.jobs[step.id]?.jobID ?? "待返回")"))
            try await persist(next)
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
