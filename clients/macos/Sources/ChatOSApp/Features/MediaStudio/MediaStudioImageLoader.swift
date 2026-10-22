import ChatOSAPI
import ChatOSCore
import Darwin
import Foundation
import ImageIO
import UniformTypeIdentifiers

enum MediaStudioImageLoader {
    static let maximumImageBytes = 20 * 1_024 * 1_024
    static let maximumEncodedCharacters = ((maximumImageBytes + 2) / 3) * 4
    private static let maximumSourcePixelCount = 64_000_000
    private static let maximumNormalizedPixelSize = 8_192

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

    static func normalizedPNGData(from data: Data) async throws -> Data {
        guard !data.isEmpty, data.count <= maximumImageBytes else {
            throw ImageError.invalidImage
        }
        let task = Task.detached(priority: .userInitiated) {
            try Task.checkCancellation()
            guard let decoded = AppImageThumbnailLoader.decode(
                data,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumNormalizedPixelSize
            ) else {
                throw ImageError.invalidImage
            }
            return try encodedPNG(decoded)
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }

    static func thumbnail(
        for asset: GeneratedMediaAsset,
        maximumDisplayPixelSize: Int,
        transport: any HTTPTransport = URLSessionHTTPTransport()
    ) async throws -> AppDecodedThumbnail {
        guard maximumDisplayPixelSize > 0 else { throw ImageError.invalidImage }
        if let url = asset.url, url.isFileURL {
            let owner = asset.mediaFile
            defer { withExtendedLifetime(owner) {} }
            return try await AppImageThumbnailLoader.loadLocalFile(
                url,
                maximumBytes: maximumImageBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
        }
        let bytes = try await data(for: asset, transport: transport)
        return try await AppCancellableDetachedWork.run {
            guard let decoded = AppImageThumbnailLoader.decode(
                bytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            ) else {
                throw ImageError.invalidImage
            }
            return decoded
        }
    }

    /// Normalizes a generated image without first materializing a file-backed response as Data.
    static func normalizedPNGData(
        for asset: GeneratedMediaAsset,
        transport: any HTTPTransport = URLSessionHTTPTransport()
    ) async throws -> Data {
        if let url = asset.url, url.isFileURL {
            return try await normalizedPNGData(fromFile: url, retaining: asset.mediaFile)
        }
        let bytes = try await data(for: asset, transport: transport)
        return try await normalizedPNGData(from: bytes)
    }

    static func normalizedPNGFile(
        for asset: GeneratedMediaAsset,
        transport: any HTTPTransport = URLSessionHTTPTransport(),
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil
    ) async throws -> GeneratedMediaFile {
        let decoded = try await thumbnail(
            for: asset,
            maximumDisplayPixelSize: maximumNormalizedPixelSize,
            transport: transport
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

    static func normalizedPNGData(
        fromFile url: URL,
        retaining owner: GeneratedMediaFile? = nil
    ) async throws -> Data {
        let task = Task.detached(priority: .userInitiated) {
            try withExtendedLifetime(owner) {
                try Task.checkCancellation()
                guard let decoded = try AppImageThumbnailLoader.decodeFile(
                    url,
                    maximumBytes: maximumImageBytes,
                    maximumSourcePixelCount: maximumSourcePixelCount,
                    maximumDisplayPixelSize: maximumNormalizedPixelSize
                ) else {
                    throw ImageError.invalidImage
                }
                return try encodedPNG(decoded)
            }
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }

    private nonisolated static func encodedPNG(_ decoded: AppDecodedThumbnail) throws -> Data {
        let destinationData = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            destinationData,
            UTType.png.identifier as CFString,
            1,
            nil
        ) else {
            throw ImageError.invalidImage
        }
        CGImageDestinationAddImage(destination, decoded.image, nil)
        guard CGImageDestinationFinalize(destination) else {
            throw ImageError.invalidImage
        }
        try Task.checkCancellation()
        let png = destinationData as Data
        guard !png.isEmpty, png.count <= 64 * 1_024 * 1_024 else {
            throw ImageError.invalidImage
        }
        return png
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
