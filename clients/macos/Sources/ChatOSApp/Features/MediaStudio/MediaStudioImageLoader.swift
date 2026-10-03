import ChatOSAPI
import ChatOSCore
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
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
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
