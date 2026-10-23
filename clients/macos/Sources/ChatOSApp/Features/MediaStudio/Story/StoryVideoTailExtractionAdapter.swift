import ChatOSCore
import Foundation

struct StoryVideoTailExtractionAdapter: StoryVideoTailExtracting {
    let store: StoryProjectStore

    func extractStoryVideoTail(
        command: StoryVideoTailExtractionCommand
    ) async throws -> StoryProject {
        let url = try store.fileURL(
            command.videoFilename,
            projectID: command.projectID,
            owner: command.ownerUserID
        )
        let png = try await StoryVideoFrameExtractor.lastFramePNG(
            from: url,
            ownerUserID: command.ownerUserID
        )
        try Task.checkCancellation()
        let applied = try await store.applyActualVideoLastFrame(
            png,
            projectID: command.projectID,
            segmentID: command.segmentID,
            videoJobID: command.videoJobID,
            owner: command.ownerUserID,
            forceNew: true
        )
        return applied.0
    }
}
