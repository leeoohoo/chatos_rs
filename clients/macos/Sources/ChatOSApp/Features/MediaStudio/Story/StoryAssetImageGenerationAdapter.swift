import ChatOSCore
import Foundation

/// macOS adapter for one Story resource image generation. Provider payloads, local media bytes,
/// filenames and Store intent records never enter the ViewModel-facing application contract.
struct StoryAssetImageGenerationAdapter: StoryAssetImageGenerating {
    let store: StoryProjectStore
    let media: any MediaGenerationServicing

    func generateStoryAssetImage(
        ownerUserID: String,
        command: StoryAssetImageGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryAssetImageGenerationEvent) async -> Void
    ) async throws -> StoryAssetImageGenerationOutcome {
        try await continuationCheck()
        let intent = try await store.beginAssetImageGeneration(
            projectID: command.projectID,
            resourceID: command.resourceID,
            attemptID: command.attemptID,
            owner: ownerUserID
        )
        await onEvent(.init(project: intent.project))
        try await continuationCheck()

        let service: any MediaGenerationServicing
        if let bindable = media as? any SessionBoundMediaGenerationServicing {
            service = try await bindable.boundToCurrentSession()
        } else {
            service = media
        }
        try await continuationCheck()
        let result = try await service.generateImage(.init(
            modelConfigID: intent.project.models.imageModelID,
            prompt: StoryGenerationContext.assetPrompt(
                intent.project,
                resource: intent.resource,
                userIdeas: command.userIdeas
            ),
            size: nil,
            count: 1,
            clientRequestID: command.attemptID.uuidString,
            projectID: command.projectID.uuidString,
            resourceID: command.resourceID
        ))
        try await continuationCheck()
        guard result.clientRequestID == nil
                || result.clientRequestID == command.attemptID.uuidString,
              result.projectID == nil || result.projectID == command.projectID.uuidString,
              result.resourceID == nil || result.resourceID == command.resourceID,
              let generated = result.images.first else { throw StoryError.invalidPlan }
        let pngFile = try await MediaStudioImageLoader.normalizedPNGFile(for: generated)
        try await continuationCheck()
        let completed = try await store.completeAssetImageGeneration(
            pngFile,
            mimeType: "image/png",
            projectID: command.projectID,
            resourceID: command.resourceID,
            attemptID: command.attemptID,
            providerResultID: result.id,
            providerAssetID: generated.id,
            owner: ownerUserID
        )
        try await continuationCheck()
        return .init(project: completed.0)
    }
}
