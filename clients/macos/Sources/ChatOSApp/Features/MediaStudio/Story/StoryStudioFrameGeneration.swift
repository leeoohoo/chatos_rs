import AppKit
import ChatOSAgentRuntime
import ChatOSCore
import Foundation

@MainActor
extension StoryStudioViewModel {
    func generateFirstFrame(_ id: String, userIdeas: String = "") {
        guard let segment = project?.segments.first(where: { $0.id == id }) else { return }
        generateFirstFrame(id, referenceAssetIDs: segment.resourceIDs, userIdeas: userIdeas)
    }

    func generateFirstFrame(_ id: String, referenceAssetIDs requestedReferenceAssetIDs: [String],
                            userIdeas: String = "") {
        generateFrame(id, role: .first, referenceAssetIDs: requestedReferenceAssetIDs,
                      userIdeas: userIdeas)
    }

    func generateLastFrame(_ id: String, userIdeas: String = "") {
        guard let segment = project?.segments.first(where: { $0.id == id }) else { return }
        generateFrame(id, role: .last, referenceAssetIDs: segment.resourceIDs, userIdeas: userIdeas)
    }

    func generateFrame(_ id: String, role: StoryFrameRole,
                       referenceAssetIDs requestedReferenceAssetIDs: [String], userIdeas: String = "") {
        guard !isBusy, !isLoading, activeAssetGenerations.isEmpty, activeFrameGenerations.isEmpty,
              let owner, let project,
              let segment = project.segments.first(where: { $0.id == id }),
              segment.detail != nil, segment.attempt == nil else { return }
        let referenceAssetIDs = segment.resourceIDs.filter(requestedReferenceAssetIDs.contains)
        guard !referenceAssetIDs.isEmpty, Set(referenceAssetIDs) == Set(requestedReferenceAssetIDs) else {
            errorMessage = "请选择至少一个属于当前分段且已确认图片的素材。"
            return
        }
        let frameAttempt = role == .first ? segment.firstFrames.generationAttemptID : segment.lastFrames.generationAttemptID
        guard frameAttempt == nil else { errorMessage = StoryError.unresolvedSubmission.localizedDescription; return }
        let key = FrameGenerationKey(projectID: project.id, segmentID: id, role: role)
        let attemptID = UUID()
        let token = session
        activeFrameGenerations.insert(key)
        frameGenerationErrors[key] = nil
        let generationTask = Task { [weak self] in
            guard let self else { return }
            await self.performFrameGeneration(
                key: key, role: role, referenceAssetIDs: referenceAssetIDs,
                attemptID: attemptID, owner: owner, token: token,
                project: project, segment: segment, userIdeas: userIdeas
            )
        }
        frameGenerationTasks[key] = generationTask
    }

    func performFrameGeneration(key: FrameGenerationKey, role: StoryFrameRole,
                                        referenceAssetIDs: [String], attemptID: UUID,
                                        owner: String, token: UUID, project: StoryProject,
                                        segment: StorySegment, userIdeas: String) async {
        defer {
            if session == token {
                activeFrameGenerations.remove(key)
                frameGenerationTasks[key] = nil
                clearImageGenerationLockNoticeIfNeeded()
            }
        }
        do {
            try check(token)
            let service = try await boundMedia(token)
            var references: [ImageGenerationInputImage] = []
            for assetID in referenceAssetIDs {
                guard let asset = project.resource(id: assetID), let image = asset.confirmedImage else { throw StoryError.invalidPlan }
                let url = try store.fileURL(image.filename, projectID: project.id, owner: owner)
                let data = try await MediaStudioImageLoader.data(for: .init(id: image.id.uuidString, mimeType: image.mimeType, url: url))
                references.append(.init(name: asset.name + ".png", mimeType: image.mimeType, base64Data: data.base64EncodedString()))
            }
            var previousTailReferenceIndex: Int?
            var currentFirstFrameReferenceIndex: Int?
            if role == .first, let previous = StoryContinuityContext.previousTail(project, segmentID: key.segmentID) {
                let url = try store.fileURL(previous.image.filename, projectID: project.id, owner: owner)
                let data = try await MediaStudioImageLoader.data(for: .init(
                    id: previous.image.id.uuidString, mimeType: previous.image.mimeType, url: url
                ))
                references.append(.init(name: "previous-segment-last-frame.png",
                                        mimeType: previous.image.mimeType,
                                        base64Data: data.base64EncodedString()))
                previousTailReferenceIndex = references.count
            } else if role == .last, let firstFrame = segment.firstFrame {
                let url = try store.fileURL(firstFrame.filename, projectID: project.id, owner: owner)
                let data = try await MediaStudioImageLoader.data(for: .init(
                    id: firstFrame.id.uuidString, mimeType: firstFrame.mimeType, url: url
                ))
                references.append(.init(name: "current-segment-first-frame.png",
                                        mimeType: firstFrame.mimeType,
                                        base64Data: data.base64EncodedString()))
                currentFirstFrameReferenceIndex = references.count
            }
            try check(token)
            let intent = try await store.beginFrameImageGeneration(projectID: project.id,
                                                                    segmentID: key.segmentID,
                                                                    role: role, attemptID: attemptID,
                                                                    owner: owner)
            try check(token)
            publishProject(intent.project, token: token)
            let result = try await service.generateImage(.init(modelConfigID: project.models.imageModelID,
                prompt: StoryGenerationContext.framePrompt(intent.project, segment: intent.segment, role: role,
                                                           referenceResourceIDs: referenceAssetIDs,
                                                           previousTailReferenceIndex: previousTailReferenceIndex,
                                                           currentFirstFrameReferenceIndex: currentFirstFrameReferenceIndex,
                                                           userIdeas: userIdeas),
                size: nil, count: 1, referenceImages: references,
                clientRequestID: attemptID.uuidString, projectID: project.id.uuidString,
                resourceID: "\(key.segmentID):\(role.rawValue)"))
            try check(token)
            guard result.clientRequestID == nil || result.clientRequestID == attemptID.uuidString,
                  result.projectID == nil || result.projectID == project.id.uuidString,
                  result.resourceID == nil || result.resourceID == "\(key.segmentID):\(role.rawValue)" else {
                throw StoryError.invalidPlan
            }
            guard let image = result.images.first else { throw StoryError.unsafeFile }
            let data = try await MediaStudioImageLoader.data(for: image)
            guard let nsImage = NSImage(data: data), let tiff = nsImage.tiffRepresentation,
                  let bitmap = NSBitmapImageRep(data: tiff),
                  let png = bitmap.representation(using: .png, properties: [:]) else { throw StoryError.unsafeFile }
            let completed = try await store.completeFrameImageGeneration(
                png, mimeType: "image/png", projectID: project.id, segmentID: key.segmentID, role: role,
                attemptID: attemptID, providerResultID: result.id, providerAssetID: image.id, owner: owner
            )
            try check(token)
            publishProject(completed.0, token: token)
        } catch is CancellationError {
            // Keep a provider-owned attempt unresolved when the app/session is interrupted.
        } catch {
            guard session == token else { return }
            frameGenerationErrors[key] = error.localizedDescription
        }
    }

}
