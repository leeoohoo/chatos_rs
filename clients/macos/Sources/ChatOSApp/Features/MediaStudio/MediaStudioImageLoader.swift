import ChatOSAPI
import ChatOSCore
import ChatOSNetworking
import Darwin
import Foundation
import ImageIO
import UniformTypeIdentifiers

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
        maximumDisplayPixelSize: Int,
        transport: any HTTPTransport = URLSessionHTTPTransport(),
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared
    ) async throws -> AppDecodedThumbnail {
        guard maximumDisplayPixelSize > 0 else { throw ImageError.invalidImage }
        let source = try await preparedSource(
            for: asset,
            transport: transport,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        )
        // Small cached thumbnails are latency-sensitive and already have a bounded decoded
        // footprint. Reserve the single heavy-media lane for large previews and normalization.
        guard maximumDisplayPixelSize > 2_048 else {
            return try await decode(
                source,
                maximumDisplayPixelSize: maximumDisplayPixelSize
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
                maximumDisplayPixelSize: maximumDisplayPixelSize
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
        transport: any HTTPTransport = URLSessionHTTPTransport(),
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared
    ) async throws -> GeneratedMediaFile {
        let source = try await preparedSource(
            for: asset,
            transport: transport,
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
            let decoded = try await decode(
                source,
                maximumDisplayPixelSize: maximumNormalizedPixelSize
            )
            return try await ChatOSMediaArtifactFileWriter.writeNormalizedImage(
                maximumBytes: maximumImageBytes,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride
            ) { destinationHandle in
                try encodedPNG(
                    decoded,
                    to: destinationHandle,
                    maximumBytes: maximumImageBytes
                )
            }
        }
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
        maximumDisplayPixelSize: Int
    ) async throws -> AppDecodedThumbnail {
        if let fileURL = source.fileURL {
            defer { withExtendedLifetime(source.owner) {} }
            return try await AppImageThumbnailLoader.loadLocalFile(
                fileURL,
                maximumBytes: maximumImageBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
        }
        guard let data = source.data else { throw ImageError.invalidImage }
        return try await AppCancellableDetachedWork.run {
            guard let decoded = AppImageThumbnailLoader.decode(
                data,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            ) else {
                throw ImageError.invalidImage
            }
            return decoded
        }
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

    private nonisolated static func encodedPNG(
        _ decoded: AppDecodedThumbnail,
        to destinationHandle: FileHandle,
        maximumBytes: Int
    ) throws {
        let state = ImageDataConsumerState(
            descriptor: destinationHandle.fileDescriptor,
            maximumBytes: maximumBytes
        )
        var callbacks = CGDataConsumerCallbacks(
            putBytes: { info, buffer, count in
                guard let info else { return 0 }
                return Unmanaged<ImageDataConsumerState>
                    .fromOpaque(info)
                    .takeUnretainedValue()
                    .write(buffer, count: count)
            },
            releaseConsumer: nil
        )
        guard let consumer = CGDataConsumer(
            info: Unmanaged.passUnretained(state).toOpaque(),
            cbks: &callbacks
        ), let destination = CGImageDestinationCreateWithDataConsumer(
            consumer,
            UTType.png.identifier as CFString,
            1,
            nil
        ) else {
            throw ImageError.invalidImage
        }
        CGImageDestinationAddImage(destination, decoded.image, nil)
        let finalized = CGImageDestinationFinalize(destination)
        try withExtendedLifetime(state) {
            if let error = state.error { throw error }
        }
        guard finalized else {
            throw ImageError.invalidImage
        }
        try Task.checkCancellation()
    }

    private final class ImageDataConsumerState {
        let descriptor: Int32
        let maximumBytes: Int
        private(set) var error: Error?
        private var bytesWritten = 0

        init(descriptor: Int32, maximumBytes: Int) {
            self.descriptor = descriptor
            self.maximumBytes = maximumBytes
        }

        func write(_ buffer: UnsafeRawPointer, count: Int) -> Int {
            guard error == nil else { return 0 }
            guard count > 0 else { return 0 }
            guard count <= maximumBytes - bytesWritten else {
                error = MediaStudioImageLoader.ImageError.invalidImage
                return 0
            }
            var written = 0
            while written < count {
                let result = Darwin.write(
                    descriptor,
                    buffer.advanced(by: written),
                    count - written
                )
                if result > 0 {
                    written += result
                    bytesWritten += result
                } else if result < 0, errno == EINTR {
                    continue
                } else {
                    error = NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
                    return written
                }
            }
            return written
        }
    }

    enum ImageError: LocalizedError {
        case invalidImage, downloadFailed
        var errorDescription: String? {
            switch self {
            case .invalidImage: "无法读取图片，文件可能已丢失、损坏或超过 20 MB。"
            case .downloadFailed: "无法下载这张图片，请稍后重试。"
            }
        }
    }
}
