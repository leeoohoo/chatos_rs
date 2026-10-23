import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// macOS adapter for Story first/last-frame generation. Reference media files, filenames,
/// prompt payloads and durable Store intents remain behind the application Port.
struct StoryFrameImageGenerationAdapter: StoryFrameImageGenerating {
    let store: StoryProjectStore
    let media: any MediaGenerationServicing

    func generateStoryFrameImage(
        ownerUserID: String,
        command: StoryFrameImageGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryFrameImageGenerationEvent) async -> Void
    ) async throws -> StoryFrameImageGenerationOutcome {
        try await continuationCheck()
        let service: any MediaGenerationServicing
        if let bindable = media as? any SessionBoundMediaGenerationServicing {
            service = try await bindable.boundToCurrentSession()
        } else {
            service = media
        }
        try await continuationCheck()

        guard let segment = command.project.segments.first(where: {
            $0.id == command.segmentID
        }) else { throw StoryError.invalidPlan }
        var references: [ImageGenerationInputImage] = []
        for resourceID in command.referenceResourceIDs {
            guard let resource = command.project.resource(id: resourceID),
                  let image = resource.confirmedImage else { throw StoryError.invalidPlan }
            let url = try store.fileURL(
                image.filename,
                projectID: command.projectID,
                owner: ownerUserID
            )
            try await continuationCheck()
            references.append(.init(
                name: resource.name + ".png",
                mimeType: image.mimeType,
                fileURL: url
            ))
        }

        var previousTailReferenceIndex: Int?
        var currentFirstFrameReferenceIndex: Int?
        if command.role == .first,
           let previous = StoryContinuityContext.previousTail(
            command.project,
            segmentID: command.segmentID
           ) {
            let url = try store.fileURL(
                previous.image.filename,
                projectID: command.projectID,
                owner: ownerUserID
            )
            try await continuationCheck()
            references.append(.init(
                name: "previous-segment-last-frame.png",
                mimeType: previous.image.mimeType,
                fileURL: url
            ))
            previousTailReferenceIndex = references.count
        } else if command.role == .last, let firstFrame = segment.firstFrame {
            let url = try store.fileURL(
                firstFrame.filename,
                projectID: command.projectID,
                owner: ownerUserID
            )
            try await continuationCheck()
            references.append(.init(
                name: "current-segment-first-frame.png",
                mimeType: firstFrame.mimeType,
                fileURL: url
            ))
            currentFirstFrameReferenceIndex = references.count
        }

        try await continuationCheck()
        let intent = try await store.beginFrameImageGeneration(
            projectID: command.projectID,
            segmentID: command.segmentID,
            role: command.role,
            attemptID: command.attemptID,
            owner: ownerUserID
        )
        await onEvent(.init(project: intent.project))
        try await continuationCheck()
        let result = try await service.generateImage(.init(
            modelConfigID: command.project.models.imageModelID,
            prompt: StoryGenerationContext.framePrompt(
                intent.project,
                segment: intent.segment,
                role: command.role,
                referenceResourceIDs: command.referenceResourceIDs,
                previousTailReferenceIndex: previousTailReferenceIndex,
                currentFirstFrameReferenceIndex: currentFirstFrameReferenceIndex,
                userIdeas: command.userIdeas
            ),
            size: nil,
            count: 1,
            referenceImages: references,
            clientRequestID: command.attemptID.uuidString,
            projectID: command.projectID.uuidString,
            resourceID: "\(command.segmentID):\(command.role.rawValue)"
        ))
        try await continuationCheck()
        guard result.clientRequestID == nil
                || result.clientRequestID == command.attemptID.uuidString,
              result.projectID == nil || result.projectID == command.projectID.uuidString,
              result.resourceID == nil
                || result.resourceID == "\(command.segmentID):\(command.role.rawValue)",
              let generated = result.images.first else { throw StoryError.invalidPlan }
        let pngFile = try await MediaStudioImageLoader.normalizedPNGFile(
            for: generated,
            ownerUserID: ownerUserID
        )
        try await continuationCheck()
        let completed = try await store.completeFrameImageGeneration(
            pngFile,
            mimeType: "image/png",
            projectID: command.projectID,
            segmentID: command.segmentID,
            role: command.role,
            attemptID: command.attemptID,
            providerResultID: result.id,
            providerAssetID: generated.id,
            owner: ownerUserID
        )
        try await continuationCheck()
        return .init(project: completed.0)
    }
}
