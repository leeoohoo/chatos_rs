import ChatOSCore
import Foundation

struct StoryImageImportAdapter: StoryImageImporting {
    private static let maximumLocalFileBytes = 20 * 1_024 * 1_024

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
        let data = try await AppCancellableDetachedWork.run {
            let access = fileURL.startAccessingSecurityScopedResource()
            defer { if access { fileURL.stopAccessingSecurityScopedResource() } }
            let values = try fileURL.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey])
            guard values.isRegularFile == true,
                  (values.fileSize ?? 0) <= Self.maximumLocalFileBytes else {
                throw StoryError.unsafeFile
            }
            return try AppBoundedFileReader.read(
                fileURL,
                maximumBytes: Self.maximumLocalFileBytes
            )
        }
        return try await importImageData(data, command: command)
    }

    private func importImageData(
        _ data: Data,
        command: StoryImageImportCommand
    ) async throws -> StoryProject {
        try Task.checkCancellation()
        let png = try await MediaStudioImageLoader.normalizedPNGData(from: data)
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
