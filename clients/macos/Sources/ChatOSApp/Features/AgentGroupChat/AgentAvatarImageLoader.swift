import ChatOSAPI
import ChatOSCore
import ChatOSMediaWorkerClient
import ChatOSNetworking
import Foundation

enum AgentAvatarImageLoader {
    static func normalizedData(
        for asset: GeneratedMediaAsset,
        ownerUserID: String,
        transport: any HTTPTransport = URLSessionHTTPTransport(),
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared
    ) async throws -> Data {
        guard !ownerUserID.isEmpty else { throw AgentAvatarImageError.invalidImage }
        if let url = asset.url, url.isFileURL {
            let owner = asset.mediaFile
            defer { withExtendedLifetime(owner) {} }
            return try await normalizedData(
                forFile: url,
                ownerUserID: ownerUserID,
                taskID: asset.id,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride,
                heavyMediaGovernor: heavyMediaGovernor,
                mediaWorker: mediaWorker
            )
        }
        if let url = asset.url,
           url.scheme?.lowercased() == "https" {
            let downloaded = try await MediaStudioImageLoader.downloadedFile(
                from: url,
                transport: transport,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride
            )
            defer { withExtendedLifetime(downloaded) {} }
            return try await normalizedData(
                forFile: downloaded.url,
                ownerUserID: ownerUserID,
                taskID: asset.id,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride,
                heavyMediaGovernor: heavyMediaGovernor,
                mediaWorker: mediaWorker
            )
        }
        let file = try await materializeInlineSource(
            asset,
            transport: transport,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        )
        defer { withExtendedLifetime(file) {} }
        return try await normalizedData(
            forFile: file.url,
            ownerUserID: ownerUserID,
            taskID: asset.id,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride,
            heavyMediaGovernor: heavyMediaGovernor,
            mediaWorker: mediaWorker
        )
    }

    private static func materializeInlineSource(
        _ asset: GeneratedMediaAsset,
        transport: any HTTPTransport,
        temporaryDirectory: URL,
        availableCapacityOverride: Int64?
    ) async throws -> GeneratedMediaFile {
        let source = try await MediaStudioImageLoader.data(for: asset, transport: transport)
        let body = AsyncThrowingStream<Data, Error> { continuation in
            continuation.yield(source)
            continuation.finish()
        }
        let file = try await ChatOSMediaArtifactFileWriter.writeDownloadedImage(
            body,
            maximumBytes: AgentAvatarImageProcessor.maximumInputBytes,
            expectedBytes: source.count,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        )
        return file
    }

    static func normalizedData(
        forFile url: URL,
        ownerUserID: String,
        taskID: String = UUID().uuidString.lowercased(),
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared
    ) async throws -> Data {
        guard !ownerUserID.isEmpty else { throw AgentAvatarImageError.invalidImage }
        return try await heavyMediaGovernor.perform(context: .init(
            taskID: "agent-avatar:\(String(taskID.prefix(128)))",
            kind: .imageDecode,
            estimatedDecodedBytes: AgentAvatarImageProcessor.maximumDecodePixelSize
                * AgentAvatarImageProcessor.maximumDecodePixelSize * 4
        )) {
            let decoded = try await AppImageThumbnailLoader.loadLocalFileUsingWorker(
                url,
                ownerUserID: ownerUserID,
                maximumBytes: AgentAvatarImageProcessor.maximumInputBytes,
                maximumSourcePixelCount: AgentAvatarImageProcessor.maximumSourcePixelCount,
                maximumDisplayPixelSize: AgentAvatarImageProcessor.maximumDecodePixelSize,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride,
                mediaWorker: mediaWorker
            )
            return try await AppCancellableDetachedWork.run {
                try AgentAvatarImageProcessor.normalizeDecoded(decoded.image)
            }
        }
    }
}
