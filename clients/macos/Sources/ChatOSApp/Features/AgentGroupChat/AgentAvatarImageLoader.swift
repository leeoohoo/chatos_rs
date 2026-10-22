import ChatOSAPI
import ChatOSCore
import Foundation

enum AgentAvatarImageLoader {
    static func normalizedData(
        for asset: GeneratedMediaAsset,
        transport: any HTTPTransport = URLSessionHTTPTransport(),
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil
    ) async throws -> Data {
        if let url = asset.url, url.isFileURL {
            let owner = asset.mediaFile
            defer { withExtendedLifetime(owner) {} }
            return try await normalizeFile(url)
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
            return try await normalizeFile(downloaded.url)
        }
        let source = try await MediaStudioImageLoader.data(for: asset, transport: transport)
        return try await AppCancellableDetachedWork.run {
            try AgentAvatarImageProcessor.normalize(source)
        }
    }

    private static func normalizeFile(_ url: URL) async throws -> Data {
        try await AppCancellableDetachedWork.run {
            try AgentAvatarImageProcessor.normalizeFile(url)
        }
    }
}
