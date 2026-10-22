import ChatOSCore
import Foundation

struct StoryImageImportAdapter: StoryImageImporting {
    let store: StoryProjectStore
    private let confirmations = StoryMediaConfirmationUseCase()

    func importGeneratedStoryImage(
        command: StoryImageImportCommand,
        asset: GeneratedMediaAsset
    ) async throws -> StoryProject {
        let png = try await MediaStudioImageLoader.normalizedPNGData(for: asset)
        return try await persistNormalizedImage(png, command: command)
    }

    func importLocalStoryImage(
        command: StoryImageImportCommand,
        fileURL: URL
    ) async throws -> StoryProject {
        let png: Data
        do {
            let access = fileURL.startAccessingSecurityScopedResource()
            defer { if access { fileURL.stopAccessingSecurityScopedResource() } }
            png = try await MediaStudioImageLoader.normalizedPNGData(fromFile: fileURL)
        }
        return try await persistNormalizedImage(png, command: command)
    }

    private func persistNormalizedImage(
        _ png: Data,
        command: StoryImageImportCommand
    ) async throws -> StoryProject {
        try Task.checkCancellation()
        let stored = try await store.saveImage(
            png,
            mimeType: "image/png",
            projectID: command.project.id,
            owner: command.ownerUserID
        )
        try Task.checkCancellation()

        let next = try confirmations.attachingImportedImage(
            stored,
            target: command.target,
            confirmsImportedImage: command.confirmsImportedImage,
            to: command.project
        )
        return try await store.persistStoryProject(
            next,
            ownerUserID: command.ownerUserID,
            preservingVideoSegmentIDs: command.preservingVideoSegmentIDs
        )
    }
}
