import ChatOSAPI
import ChatOSCore
import ChatOSNetworking
import Foundation

enum MediaStudioImageLoader {
    static let maximumImageBytes = 20 * 1_024 * 1_024
    static let maximumEncodedCharacters = ((maximumImageBytes + 2) / 3) * 4
    private static let maximumSourcePixelCount = 64_000_000
    private static let maximumNormalizedPixelSize = 8_192

    private struct PreparedImageSource: Sendable {
        let fileURL: URL?
        let owner: GeneratedMediaFile?
        let data: Data?
    }

    static func data(for asset: GeneratedMediaAsset, transport: any HTTPTransport = URLSessionHTTPTransport()) async throws -> Data {
        try Task.checkCancellation()
        let data: Data
        if let base64 = asset.base64Data,
           base64.utf8.count <= maximumEncodedCharacters,
           let decoded = Data(base64Encoded: base64),
           decoded.count <= maximumImageBytes {
            data = decoded
        } else if let url = asset.url, url.isFileURL {
            let task = Task.detached(priority: .userInitiated) {
                try Task.checkCancellation()
                let values = try url.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey])
                guard values.isRegularFile == true,
                      (values.fileSize ?? 0) <= maximumImageBytes else {
                    throw ImageError.invalidImage
                }
                return try AppBoundedFileReader.read(
                    url,
                    maximumBytes: maximumImageBytes
                )
            }
            data = try await withTaskCancellationHandler {
                try await task.value
            } onCancel: {
                task.cancel()
            }
        } else if let url = asset.url, url.scheme?.lowercased() == "https", url.host != nil,
                  url.user == nil, url.password == nil {
            // Generated CDN URLs authorize themselves; do not send a model API key.
            let response = try await transport.send(.init(
                url: url, method: "GET", headers: ["Accept": "image/*"], timeoutInterval: 60,
                maximumResponseBytes: 20 * 1_024 * 1_024
            ))
            guard (200..<300).contains(response.statusCode) else { throw ImageError.downloadFailed }
            data = response.body
        } else {
            throw ImageError.invalidImage
        }
        try Task.checkCancellation()
        guard !data.isEmpty, data.count <= 20 * 1024 * 1024 else { throw ImageError.invalidImage }
        return data
    }

    static func thumbnail(
        for asset: GeneratedMediaAsset,
        ownerUserID: String,
        maximumDisplayPixelSize: Int,
        transport: any HTTPTransport = URLSessionHTTPTransport(),
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared
    ) async throws -> AppDecodedThumbnail {
        guard !ownerUserID.isEmpty,
              maximumDisplayPixelSize > 0 else { throw ImageError.invalidImage }
        let prepared = try await preparedSource(
            for: asset,
            transport: transport,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        )
        let source = try await fileBackedSource(
            prepared,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        )
        // Small cached thumbnails remain latency-sensitive and have a bounded output surface.
        // They still use the Worker, but do not wait behind large preview/normalization jobs.
        guard maximumDisplayPixelSize > 2_048 else {
            return try await decode(
                source,
                ownerUserID: ownerUserID,
                maximumDisplayPixelSize: maximumDisplayPixelSize,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride,
                mediaWorker: mediaWorker
            )
        }
        return try await heavyMediaGovernor.perform(context: .init(
            taskID: mediaTaskID(assetID: asset.id, operation: "thumbnail"),
            kind: .imageDecode,
            estimatedDecodedBytes: estimatedDecodedBytes(
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
        )) {
            try await decode(
                source,
                ownerUserID: ownerUserID,
                maximumDisplayPixelSize: maximumDisplayPixelSize,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride,
                mediaWorker: mediaWorker
            )
        }
    }

    static func downloadedFile(
        from url: URL,
        transport: any HTTPTransport = URLSessionHTTPTransport(),
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil
    ) async throws -> GeneratedMediaFile {
        guard url.scheme?.lowercased() == "https", url.host != nil,
              url.user == nil, url.password == nil else {
            throw ImageError.invalidImage
        }
        let response = try await transport.stream(.init(
            url: url,
            method: "GET",
            headers: ["Accept": "image/*"],
            timeoutInterval: 120,
            maximumResponseBytes: maximumImageBytes
        ))
        guard (200..<300).contains(response.statusCode) else {
            throw ImageError.downloadFailed
        }
        let contentEncoding = response.headers.first(where: {
            $0.key.lowercased() == "content-encoding"
        })?.value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let expectedBytes: Int?
        if contentEncoding == nil || contentEncoding == "identity",
           let rawLength = response.headers.first(where: {
               $0.key.lowercased() == "content-length"
           })?.value {
            guard let length = Int(rawLength.trimmingCharacters(in: .whitespacesAndNewlines)),
                  length > 0,
                  length <= maximumImageBytes else {
                throw ImageError.invalidImage
            }
            expectedBytes = length
        } else {
            expectedBytes = nil
        }
        return try await ChatOSMediaArtifactFileWriter.writeDownloadedImage(
            response.body,
            maximumBytes: maximumImageBytes,
            expectedBytes: expectedBytes,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        )
    }

    static func normalizedPNGFile(
        for asset: GeneratedMediaAsset,
        ownerUserID: String,
        transport: any HTTPTransport = URLSessionHTTPTransport(),
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared
    ) async throws -> GeneratedMediaFile {
        let prepared = try await preparedSource(
            for: asset,
            transport: transport,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        )
        let source = try await fileBackedSource(
            prepared,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        )
        return try await heavyMediaGovernor.perform(context: .init(
            taskID: mediaTaskID(assetID: asset.id, operation: "normalize"),
            kind: .imageDecode,
            estimatedDecodedBytes: estimatedDecodedBytes(
                maximumDisplayPixelSize: maximumNormalizedPixelSize
            )
        )) {
            defer { withExtendedLifetime(source.owner) {} }
            guard let sourceURL = source.fileURL else { throw ImageError.invalidImage }
            return try await ChatOSMediaArtifactFileWriter.writeNormalizedImageUsingWorker(
                maximumBytes: maximumImageBytes,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride
            ) { destinationURL, _ in
                do {
                    let descriptor = try await mediaWorker.normalizePNG(
                        sourceURL: sourceURL,
                        sourceIdentity: nil,
                        destinationURL: destinationURL,
                        ownerUserID: ownerUserID,
                        maximumSourceBytes: maximumImageBytes,
                        maximumSourcePixelCount: maximumSourcePixelCount,
                        maximumOutputPixelSize: maximumNormalizedPixelSize,
                        maximumOutputBytes: maximumImageBytes
                    )
                    guard descriptor.path == destinationURL.path,
                          descriptor.mimeType == "image/png",
                          descriptor.byteCount > 0,
                          descriptor.byteCount <= maximumImageBytes else {
                        throw ImageError.invalidImage
                    }
                } catch is CancellationError {
                    throw CancellationError()
                } catch let error as MediaWorkerProcessClientError {
                    if case let .requestRejected(code) = error,
                       code == "insufficient_disk_space" {
                        throw ImageError.insufficientDiskSpace
                    }
                    if case .requestRejected = error {
                        throw ImageError.invalidImage
                    }
                    throw error
                }
            }
        }
    }

    private static func fileBackedSource(
        _ source: PreparedImageSource,
        temporaryDirectory: URL,
        availableCapacityOverride: Int64?
    ) async throws -> PreparedImageSource {
        if source.fileURL != nil { return source }
        guard let data = source.data, !data.isEmpty, data.count <= maximumImageBytes else {
            throw ImageError.invalidImage
        }
        let body = AsyncThrowingStream<Data, Error> { continuation in
            continuation.yield(data)
            continuation.finish()
        }
        let file = try await ChatOSMediaArtifactFileWriter.writeDownloadedImage(
            body,
            maximumBytes: maximumImageBytes,
            expectedBytes: data.count,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        )
        return .init(fileURL: file.url, owner: file, data: nil)
    }

    private static func preparedSource(
        for asset: GeneratedMediaAsset,
        transport: any HTTPTransport,
        temporaryDirectory: URL,
        availableCapacityOverride: Int64?
    ) async throws -> PreparedImageSource {
        if let url = asset.url, url.isFileURL {
            return .init(fileURL: url, owner: asset.mediaFile, data: nil)
        }
        if let url = asset.url, url.scheme?.lowercased() == "https" {
            let downloaded = try await downloadedFile(
                from: url,
                transport: transport,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride
            )
            return .init(fileURL: downloaded.url, owner: downloaded, data: nil)
        }
        return .init(
            fileURL: nil,
            owner: nil,
            data: try await data(for: asset, transport: transport)
        )
    }

    private static func decode(
        _ source: PreparedImageSource,
        ownerUserID: String,
        maximumDisplayPixelSize: Int,
        temporaryDirectory: URL,
        availableCapacityOverride: Int64?,
        mediaWorker: any MediaWorkerImageNormalizing
    ) async throws -> AppDecodedThumbnail {
        defer { withExtendedLifetime(source.owner) {} }
        guard let fileURL = source.fileURL else { throw ImageError.invalidImage }
        return try await AppImageThumbnailLoader.loadLocalFileUsingWorker(
            fileURL,
            ownerUserID: ownerUserID,
            maximumBytes: maximumImageBytes,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumDisplayPixelSize: maximumDisplayPixelSize,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride,
            mediaWorker: mediaWorker
        )
    }

    private static func mediaTaskID(assetID: String, operation: String) -> String {
        let component = assetID.isEmpty
            ? UUID().uuidString.lowercased()
            : String(assetID.prefix(128))
        return "media-image:\(component):\(operation)"
    }

    private static func estimatedDecodedBytes(maximumDisplayPixelSize: Int) -> Int {
        let maximumDisplayPixels = Double(maximumDisplayPixelSize)
            * Double(maximumDisplayPixelSize)
        let decodedPixels = min(Double(maximumSourcePixelCount), maximumDisplayPixels)
        return Int(decodedPixels) * 4
    }

    enum ImageError: LocalizedError {
        case invalidImage, downloadFailed, insufficientDiskSpace
        var errorDescription: String? {
            switch self {
            case .invalidImage: "无法读取图片，文件可能已丢失、损坏或超过 20 MB。"
            case .downloadFailed: "无法下载这张图片，请稍后重试。"
            case .insufficientDiskSpace: "磁盘可用空间不足，无法处理这张图片。"
            }
        }
    }
}
