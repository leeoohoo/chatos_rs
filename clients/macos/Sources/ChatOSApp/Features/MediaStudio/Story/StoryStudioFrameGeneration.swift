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
              let owner, let project else { return }
        let key = FrameGenerationKey(projectID: project.id, segmentID: id, role: role)
        let command: StoryFrameImageGenerationCommand
        do {
            command = try storyFrameImageGeneration.prepare(
                ownerUserID: owner,
                project: project,
                segmentID: id,
                role: role,
                referenceResourceIDs: requestedReferenceAssetIDs,
                userIdeas: userIdeas
            )
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        let accountSessionID = session
        guard let request = storyFrameImageGenerationState.begin(
            ownerUserID: owner,
            accountSessionID: accountSessionID,
            command: command
        ) else { return }
        let storyFrameImageGeneration = storyFrameImageGeneration
        activeFrameGenerations.insert(key)
        frameGenerationErrors[key] = nil
        let generationTask = Task { [weak self, storyFrameImageGeneration] in
            let result: Result<StoryFrameImageGenerationOutcome, Error>
            do {
                result = .success(try await storyFrameImageGeneration.generate(
                    ownerUserID: owner,
                    command: command,
                    continuationCheck: { [weak self] in
                        guard let self else { throw CancellationError() }
                        try await self.checkFrameImageGeneration(
                            request,
                            session: accountSessionID
                        )
                    },
                    onEvent: { [weak self] event in
                        await self?.receiveFrameImageGenerationEvent(
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
                  self.storyFrameImageGenerationState.accepts(
                    request,
                    ownerUserID: self.owner,
                    accountSessionID: accountSessionID
                  ),
                  self.storyFrameImageGenerationState.finish(request) else { return }
            self.activeFrameGenerations.remove(key)
            self.frameGenerationTasks[key] = nil
            self.clearImageGenerationLockNoticeIfNeeded()
            switch result {
            case .success(let outcome):
                self.publishProject(outcome.project, token: accountSessionID)
            case .failure(let error):
                guard !(error is CancellationError), !Task.isCancelled else { return }
                self.frameGenerationErrors[key] = error.localizedDescription
            }
        }
        frameGenerationTasks[key] = generationTask
    }

    private func checkFrameImageGeneration(
        _ request: StoryFrameImageGenerationToken,
        session accountSessionID: UUID
    ) throws {
        try check(accountSessionID)
        guard storyFrameImageGenerationState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: accountSessionID
        ) else { throw CancellationError() }
    }

    private func receiveFrameImageGenerationEvent(
        _ event: StoryFrameImageGenerationEvent,
        request: StoryFrameImageGenerationToken,
        token accountSessionID: UUID
    ) {
        guard storyFrameImageGenerationState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: accountSessionID
        ) else { return }
        publishProject(event.project, token: accountSessionID)
    }

}
