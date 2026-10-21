import AppKit
import ChatOSAgentRuntime
import ChatOSCore
import Foundation

@MainActor
extension StoryStudioViewModel {
    func generateAsset(_ assetID: String, userIdeas: String = "") {
        guard !isBusy, !isLoading, let owner, let project else { return }
        let key = AssetGenerationKey(projectID: project.id, resourceID: assetID)
        let command: StoryAssetImageGenerationCommand
        do {
            command = try storyAssetImageGeneration.prepare(
                ownerUserID: owner,
                project: project,
                resourceID: assetID,
                userIdeas: userIdeas
            )
        } catch {
            assetGenerationErrors[key] = error.localizedDescription
            return
        }
        let accountSessionID = session
        guard let request = storyAssetImageGenerationState.begin(
            ownerUserID: owner,
            accountSessionID: accountSessionID,
            command: command
        ) else { return }
        let storyAssetImageGeneration = storyAssetImageGeneration
        activeAssetGenerations.insert(key)
        assetGenerationErrors[key] = nil
        let generationTask = Task { [weak self, storyAssetImageGeneration] in
            let result: Result<StoryAssetImageGenerationOutcome, Error>
            do {
                result = .success(try await storyAssetImageGeneration.generate(
                    ownerUserID: owner,
                    command: command,
                    continuationCheck: { [weak self] in
                        guard let self else { throw CancellationError() }
                        try await self.checkAssetImageGeneration(
                            request,
                            session: accountSessionID
                        )
                    },
                    onEvent: { [weak self] event in
                        await self?.receiveAssetImageGenerationEvent(
                            event,
                            request: request,
                            token: accountSessionID
                        )
                    }
                ))
            } catch {
                result = .failure(error)
            }
            guard let self,
                  self.storyAssetImageGenerationState.accepts(
                    request,
                    ownerUserID: self.owner,
                    accountSessionID: accountSessionID
                  ),
                  self.storyAssetImageGenerationState.finish(request) else { return }
            self.activeAssetGenerations.remove(key)
            self.assetGenerationTasks[key] = nil
            self.clearImageGenerationLockNoticeIfNeeded()
            switch result {
            case .success(let outcome):
                self.publishProject(outcome.project, token: accountSessionID)
            case .failure(let error):
                guard !(error is CancellationError), !Task.isCancelled else { return }
                self.assetGenerationErrors[key] = error.localizedDescription
            }
        }
        assetGenerationTasks[key] = generationTask
    }

    private func checkAssetImageGeneration(
        _ request: StoryAssetImageGenerationToken,
        session accountSessionID: UUID
    ) throws {
        try check(accountSessionID)
        guard storyAssetImageGenerationState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: accountSessionID
        ) else { throw CancellationError() }
    }

    private func receiveAssetImageGenerationEvent(
        _ event: StoryAssetImageGenerationEvent,
        request: StoryAssetImageGenerationToken,
        token accountSessionID: UUID
    ) {
        guard storyAssetImageGenerationState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: accountSessionID
        ) else { return }
        publishProject(event.project, token: accountSessionID)
    }

    func updateAssetPrompt(_ assetID: String, prompt: String) {
        guard var next = project, var resource = next.resource(id: assetID),
              !next.segments.contains(where: { $0.resourceIDs.contains(assetID) && $0.attempt != nil && $0.video == nil }) else { return }
        resource.prompt = prompt
        resource.media.confirmedImageID = nil
        do { try next.replaceResource(resource) } catch { errorMessage = error.localizedDescription; return }
        for i in next.segments.indices where next.segments[i].resourceIDs.contains(assetID) && next.segments[i].attempt == nil {
            next.segments[i].confirmedFrameID = nil
            next.segments[i].confirmedLastFrameID = nil
        }
        runCommit("保存素材描述", project: next)
    }

    func importImage(_ image: GeneratedMediaAsset, assetID: String?, segmentID: String?,
                     frameRole: StoryFrameRole = .first, confirmImported: Bool = true) {
        guard let project else { return }
        let store = store
        let preserving = Set(activeVideoGenerations.lazy.filter { $0.projectID == project.id }.map(\.segmentID))
        run("保存参考图片") { [weak self, store] owner, token in
            let saved = try await Self.attach(
                image,
                assetID: assetID,
                segmentID: segmentID,
                frameRole: frameRole,
                project: project,
                owner: owner,
                store: store,
                preservingVideoSegmentIDs: preserving,
                confirmsImportedImage: confirmImported
            )
            self?.publishProject(saved, token: token)
        }
    }

    func uploadImage(_ url: URL, assetID: String?, segmentID: String?, frameRole: StoryFrameRole = .first) {
        guard let project else { return }
        let store = store
        let preserving = Set(activeVideoGenerations.lazy.filter { $0.projectID == project.id }.map(\.segmentID))
        run("导入本机图片") { [weak self, store] owner, token in
            let data = try await AppCancellableDetachedWork.run {
                let access = url.startAccessingSecurityScopedResource()
                defer { if access { url.stopAccessingSecurityScopedResource() } }
                guard (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) <= 20 * 1024 * 1024 else { throw StoryError.unsafeFile }
                return try AppBoundedFileReader.read(
                    url,
                    maximumBytes: 20 * 1_024 * 1_024
                )
            }
            try Task.checkCancellation()
            let image = GeneratedMediaAsset(id: UUID().uuidString, mimeType: "image/png", base64Data: data.base64EncodedString())
            let saved = try await Self.attach(
                image,
                assetID: assetID,
                segmentID: segmentID,
                frameRole: frameRole,
                project: project,
                owner: owner,
                store: store,
                preservingVideoSegmentIDs: preserving
            )
            self?.publishProject(saved, token: token)
        }
    }

    nonisolated static func attach(
        _ image: GeneratedMediaAsset,
        assetID: String?,
        segmentID: String?,
        frameRole: StoryFrameRole = .first,
        project: StoryProject,
        owner: String,
        store: StoryProjectStore,
        preservingVideoSegmentIDs: Set<String>,
        clearsGenerationAttempt: Bool = false,
        confirmsImportedImage: Bool = false
    ) async throws -> StoryProject {
        let data = try await MediaStudioImageLoader.data(for: image)
        try Task.checkCancellation()
        let png = try await MediaStudioImageLoader.normalizedPNGData(from: data)
        try Task.checkCancellation()
        let stored = try await store.saveImage(
            png,
            mimeType: "image/png",
            projectID: project.id,
            owner: owner
        )
        try Task.checkCancellation()
        var next = project
        if let assetID, var resource = next.resource(id: assetID) {
            resource.media.images.append(stored)
            if clearsGenerationAttempt { resource.media.generationAttemptID = nil }
            let automaticallyConfirmed = confirmsImportedImage || resource.confirmedImageID == nil
            if automaticallyConfirmed { resource.media.confirmedImageID = stored.id }
            try next.replaceResource(resource)
            if automaticallyConfirmed {
                for index in next.segments.indices
                    where next.segments[index].resourceIDs.contains(assetID)
                        && next.segments[index].attempt == nil {
                    next.segments[index].confirmedFrameID = nil
                    next.segments[index].confirmedLastFrameID = nil
                }
            }
        } else if let segmentID, let index = next.segments.firstIndex(where: { $0.id == segmentID }), next.segments[index].attempt == nil {
            switch frameRole {
            case .first:
                next.segments[index].firstFrames.images.append(stored)
                if clearsGenerationAttempt { next.segments[index].firstFrames.generationAttemptID = nil }
                if confirmsImportedImage || next.segments[index].confirmedFrameID == nil {
                    next.segments[index].confirmedFrameID = stored.id
                    next.segments[index].userSelectedFirstFrameID = stored.id
                    next.segments[index].inheritedFirstFrameSourceSegmentID = nil
                }
            case .last:
                next.segments[index].lastFrames.images.append(stored)
                if clearsGenerationAttempt { next.segments[index].lastFrames.generationAttemptID = nil }
                if confirmsImportedImage || next.segments[index].confirmedLastFrameID == nil {
                    next.segments[index].confirmedLastFrameID = stored.id
                }
            }
        } else { throw StoryError.invalidPlan }
        return try await saveStoryProject(
            next,
            owner: owner,
            store: store,
            preservingVideoSegmentIDs: preservingVideoSegmentIDs
        )
    }

    func confirmImage(_ image: StoryImage, assetID: String?, segmentID: String?, frameRole: StoryFrameRole = .first) {
        guard var next = project else { return }
        if let assetID, var resource = next.resource(id: assetID), resource.images.contains(image) {
            guard resource.confirmedImageID != image.id else { return }
            guard !next.segments.contains(where: { $0.resourceIDs.contains(assetID) && $0.attempt != nil && $0.video == nil }) else {
                errorMessage = StoryError.unresolvedSubmission.localizedDescription; return
            }
            resource.media.confirmedImageID = image.id
            do { try next.replaceResource(resource) } catch { errorMessage = error.localizedDescription; return }
            for i in next.segments.indices where next.segments[i].resourceIDs.contains(assetID) && next.segments[i].attempt == nil {
                next.segments[i].confirmedFrameID = nil
                next.segments[i].confirmedLastFrameID = nil
            }
        } else if let segmentID, let index = next.segments.firstIndex(where: { $0.id == segmentID }), next.segments[index].attempt == nil {
            switch frameRole {
            case .first:
                guard next.segments[index].firstFrames.images.contains(image) else { return }
                next.segments[index].confirmedFrameID = image.id
                next.segments[index].userSelectedFirstFrameID = image.id
                next.segments[index].inheritedFirstFrameSourceSegmentID = nil
            case .last:
                guard next.segments[index].lastFrames.images.contains(image) else { return }
                next.segments[index].confirmedLastFrameID = image.id
            }
        } else { return }
        runCommit("确认素材版本", project: next)
    }

    func clearConfirmedLastFrame(_ segmentID: String) {
        guard var next = project,
              let index = next.segments.firstIndex(where: { $0.id == segmentID }),
              next.segments[index].attempt == nil, next.segments[index].video == nil,
              next.segments[index].confirmedLastFrameID != nil
                || next.segments[index].useLastFrameForVideo else { return }
        next.segments[index].confirmedLastFrameID = nil
        next.segments[index].useLastFrameForVideo = false
        runCommit("取消尾帧", project: next)
    }

    func confirmLatestAssetImages(_ assetIDs: [String]) {
        guard var next = project else { return }
        let requested = Set(assetIDs)
        guard !requested.isEmpty else { return }
        var changed = false
        for id in next.resources.map(\.id) where requested.contains(id) {
            guard var resource = next.resource(id: id), let latest = resource.images.last,
                  resource.confirmedImageID != latest.id else { continue }
            guard !next.segments.contains(where: {
                $0.resourceIDs.contains(id) && $0.attempt != nil && $0.video == nil
            }) else {
                errorMessage = StoryError.unresolvedSubmission.localizedDescription
                return
            }
            resource.media.confirmedImageID = latest.id
            do { try next.replaceResource(resource) } catch {
                errorMessage = error.localizedDescription
                return
            }
            for index in next.segments.indices
                where next.segments[index].resourceIDs.contains(id) && next.segments[index].attempt == nil {
                next.segments[index].confirmedFrameID = nil
                next.segments[index].confirmedLastFrameID = nil
            }
            changed = true
        }
        guard changed else { return }
        runCommit("确认已生成素材", project: next)
    }

    /// Promotes already-generated frame versions in one explicit, non-billable action.
    /// Existing confirmations are never replaced; only missing confirmations use the latest image.
    func confirmLatestFrames(_ segmentID: String, useConfirmedLastFrameForVideo: Bool) {
        guard var next = project,
              let index = next.segments.firstIndex(where: { $0.id == segmentID }),
              next.segments[index].attempt == nil, next.segments[index].video == nil else { return }
        var changed = false
        if next.segments[index].confirmedFrameID == nil,
                  let latest = next.segments[index].firstFrames.images.last {
            next.segments[index].confirmedFrameID = latest.id
            next.segments[index].userSelectedFirstFrameID = latest.id
            next.segments[index].inheritedFirstFrameSourceSegmentID = nil
            changed = true
        }
        if next.segments[index].confirmedLastFrameID == nil,
           let latest = next.segments[index].lastFrames.images.last {
            next.segments[index].confirmedLastFrameID = latest.id
            changed = true
        }
        if useConfirmedLastFrameForVideo,
           next.segments[index].confirmedLastFrameID != nil,
           !next.segments[index].useLastFrameForVideo {
            next.segments[index].useLastFrameForVideo = true
            changed = true
        }
        guard changed else { return }
        runCommit("确认首尾帧", project: next)
    }

    /// Explicit user recovery after checking that an ambiguous image submission did not produce a usable result.
    func allowImageRetryAfterVerification(assetID: String?, segmentID: String?, frameRole: StoryFrameRole = .first) {
        guard var next = project else { return }
        if let assetID, var resource = next.resource(id: assetID), resource.media.generationAttemptID != nil {
            resource.media.generationAttemptID = nil
            do { try next.replaceResource(resource) } catch { errorMessage = error.localizedDescription; return }
        } else if let segmentID, let index = next.segments.firstIndex(where: { $0.id == segmentID }) {
            switch frameRole {
            case .first:
                guard next.segments[index].firstFrames.generationAttemptID != nil else { return }
                next.segments[index].firstFrames.generationAttemptID = nil
            case .last:
                guard next.segments[index].lastFrames.generationAttemptID != nil else { return }
                next.segments[index].lastFrames.generationAttemptID = nil
            }
        } else { return }
        runCommit("核对后允许重新生成图片", project: next)
    }

}
