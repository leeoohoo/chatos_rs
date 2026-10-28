import ChatOSCore
import ChatOSMediaWorkerClient
import Foundation

struct StoryImageImportAdapter: StoryImageImporting {
    let store: StoryProjectStore
    let mediaWorker: any MediaWorkerImageNormalizing
    private let confirmations = StoryMediaConfirmationUseCase()

    init(
        store: StoryProjectStore,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared
    ) {
        self.store = store
        self.mediaWorker = mediaWorker
    }

    func importGeneratedStoryImage(
        command: StoryImageImportCommand,
        asset: GeneratedMediaAsset
    ) async throws -> StoryProject {
        let pngFile = try await MediaStudioImageLoader.normalizedPNGFile(
            for: asset,
            ownerUserID: command.ownerUserID,
            mediaWorker: mediaWorker
        )
        return try await persistNormalizedImage(pngFile, command: command)
    }

    func importLocalStoryImage(
        command: StoryImageImportCommand,
        fileURL: URL
    ) async throws -> StoryProject {
        let pngFile: GeneratedMediaFile
        do {
            let access = fileURL.startAccessingSecurityScopedResource()
            defer { if access { fileURL.stopAccessingSecurityScopedResource() } }
            pngFile = try await MediaStudioImageLoader.normalizedPNGFile(
                for: .init(
                    id: UUID().uuidString,
                    mimeType: "image/png",
                    url: fileURL
                ),
                ownerUserID: command.ownerUserID,
                mediaWorker: mediaWorker
            )
        }
        return try await persistNormalizedImage(pngFile, command: command)
    }

    private func persistNormalizedImage(
        _ pngFile: GeneratedMediaFile,
        command: StoryImageImportCommand
    ) async throws -> StoryProject {
        try Task.checkCancellation()
        let stored = try await store.saveImage(
            pngFile,
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
