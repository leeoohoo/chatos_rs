import ChatOSCore
import Foundation

struct StoryImageImportAdapter: StoryImageImporting {
    private static let maximumLocalFileBytes = 20 * 1_024 * 1_024

    let store: StoryProjectStore

    func importGeneratedStoryImage(
        command: StoryImageImportCommand,
        asset: GeneratedMediaAsset
    ) async throws -> StoryProject {
        let data = try await MediaStudioImageLoader.data(for: asset)
        return try await importImageData(data, command: command)
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
        try Task.checkCancellation()
        let stored = try await store.saveImage(
            png,
            mimeType: "image/png",
            projectID: command.project.id,
            owner: command.ownerUserID
        )
        try Task.checkCancellation()

        var next = command.project
        switch command.target {
        case .resource(let resourceID):
            guard var resource = next.resource(id: resourceID) else {
                throw StoryImageImportError.invalidTarget
            }
            resource.media.images.append(stored)
            let automaticallyConfirmed = command.confirmsImportedImage
                || resource.confirmedImageID == nil
            if automaticallyConfirmed {
                resource.media.confirmedImageID = stored.id
            }
            try next.replaceResource(resource)
            if automaticallyConfirmed {
                for index in next.segments.indices
                    where next.segments[index].resourceIDs.contains(resourceID)
                        && next.segments[index].attempt == nil {
                    next.segments[index].confirmedFrameID = nil
                    next.segments[index].confirmedLastFrameID = nil
                }
            }
        case .frame(let segmentID, let role):
            guard let index = next.segments.firstIndex(where: { $0.id == segmentID }),
                  next.segments[index].attempt == nil else {
                throw StoryImageImportError.invalidTarget
            }
            switch role {
            case .first:
                next.segments[index].firstFrames.images.append(stored)
                if command.confirmsImportedImage
                    || next.segments[index].confirmedFrameID == nil {
                    next.segments[index].confirmedFrameID = stored.id
                    next.segments[index].userSelectedFirstFrameID = stored.id
                    next.segments[index].inheritedFirstFrameSourceSegmentID = nil
                }
            case .last:
                next.segments[index].lastFrames.images.append(stored)
                if command.confirmsImportedImage
                    || next.segments[index].confirmedLastFrameID == nil {
                    next.segments[index].confirmedLastFrameID = stored.id
                }
            }
        }
        return try await store.persistStoryProject(
            next,
            ownerUserID: command.ownerUserID,
            preservingVideoSegmentIDs: command.preservingVideoSegmentIDs
        )
    }
}
