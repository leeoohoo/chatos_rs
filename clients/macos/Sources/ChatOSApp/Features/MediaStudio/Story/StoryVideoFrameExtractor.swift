import ChatOSAPI
import ChatOSCore
import ChatOSMediaWorkerClient
import ChatOSNetworking
import Foundation

enum StoryVideoFrameExtractor {
    private static let maximumFramePixelSize = 4_096
    private static let maximumVideoBytes = 512 * 1_024 * 1_024
    private static let maximumFrameBytes = 20 * 1_024 * 1_024

    static func lastFramePNG(
        from videoURL: URL,
        ownerUserID: String,
        taskID: String? = nil,
        ownerSessionID: String? = nil,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared,
        mediaWorker: any MediaWorkerVideoFrameExtracting = MediaWorkerProcessClient.shared
    ) async throws -> GeneratedMediaFile {
        guard videoURL.isFileURL, !ownerUserID.isEmpty else {
            throw StoryError.unsafeFile
        }
        let taskComponent = taskID.map { String($0.prefix(128)) }
            ?? UUID().uuidString.lowercased()
        return try await heavyMediaGovernor.perform(context: .init(
            taskID: "video:\(taskComponent):last-frame",
            ownerSessionID: ownerSessionID,
            kind: .videoFrameExtraction,
            estimatedDecodedBytes: maximumFramePixelSize * maximumFramePixelSize * 4
        )) {
            try await ChatOSMediaArtifactFileWriter.writeNormalizedImageUsingWorker(
                maximumBytes: maximumFrameBytes,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride
            ) { destinationURL, _ in
                let descriptor = try await mediaWorker.extractLastVideoFramePNG(
                    sourceURL: videoURL,
                    destinationURL: destinationURL,
                    ownerUserID: ownerUserID,
                    maximumSourceBytes: maximumVideoBytes,
                    maximumOutputPixelSize: maximumFramePixelSize,
                    maximumOutputBytes: maximumFrameBytes
                )
                guard descriptor.path == destinationURL.path,
                      descriptor.mimeType == "image/png",
                      descriptor.byteCount > 0,
                      descriptor.byteCount <= maximumFrameBytes,
                      descriptor.pixelWidth > 0,
                      descriptor.pixelHeight > 0,
                      max(descriptor.pixelWidth, descriptor.pixelHeight) <= maximumFramePixelSize else {
                    throw StoryError.unsafeFile
                }
            }
        }
    }
}
