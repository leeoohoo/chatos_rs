import ChatOSMediaWorkerClient
import Foundation
import ImageIO

struct AppPastedImageNormalizationLimits: Sendable, Equatable {
    static let attachment = Self(
        maximumInputBytes: 20 * 1_024 * 1_024,
        maximumSourcePixelCount: 64_000_000,
        maximumDecodePixelSize: 4_096,
        maximumOutputBytes: 20 * 1_024 * 1_024
    )

    let maximumInputBytes: Int
    let maximumSourcePixelCount: Int
    let maximumDecodePixelSize: Int
    let maximumOutputBytes: Int
}

enum AppPastedImageNormalizer {
    static let directlyUploadableMIMETypes: Set<String> = [
        "image/jpeg",
        "image/png",
        "image/webp",
    ]

    static func requiresPNGNormalization(mimeType: String) -> Bool {
        !directlyUploadableMIMETypes.contains(mimeType.lowercased())
    }

    static func normalizeToPNGOffMain(
        _ data: Data,
        limits: AppPastedImageNormalizationLimits = .attachment,
        ownerUserID: String,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared
    ) async throws -> Data {
        try Task.checkCancellation()
        guard !data.isEmpty,
              data.count <= limits.maximumInputBytes,
              limits.maximumInputBytes > 0,
              limits.maximumSourcePixelCount > 0,
              limits.maximumDecodePixelSize > 0,
              limits.maximumOutputBytes > 0,
              !ownerUserID.isEmpty else {
            throw AppPastedImageNormalizationError.invalidImage
        }
        let root = temporaryDirectory.appendingPathComponent(
            "ChatOSPastedImage-\(UUID().uuidString.lowercased())",
            isDirectory: true
        )
        let sourceURL = root.appendingPathComponent("source.image", isDirectory: false)
        let destinationURL = root.appendingPathComponent("normalized.png", isDirectory: false)
        do {
            try FileManager.default.createDirectory(
                at: root,
                withIntermediateDirectories: false,
                attributes: [.posixPermissions: 0o700]
            )
            try data.write(to: sourceURL, options: .withoutOverwriting)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: sourceURL.path
            )
            guard FileManager.default.createFile(
                atPath: destinationURL.path,
                contents: nil,
                attributes: [.posixPermissions: 0o600]
            ) else {
                throw AppPastedImageNormalizationError.cannotEncode
            }
        } catch {
            try? FileManager.default.removeItem(at: root)
            throw error
        }
        defer { try? FileManager.default.removeItem(at: root) }

        do {
            let descriptor = try await mediaWorker.normalizePNG(
                sourceURL: sourceURL,
                sourceIdentity: nil,
                destinationURL: destinationURL,
                ownerUserID: ownerUserID,
                maximumSourceBytes: limits.maximumInputBytes,
                maximumSourcePixelCount: limits.maximumSourcePixelCount,
                maximumOutputPixelSize: limits.maximumDecodePixelSize,
                maximumOutputBytes: limits.maximumOutputBytes
            )
            guard descriptor.path == destinationURL.path,
                  descriptor.mimeType == "image/png",
                  descriptor.byteCount > 0,
                  descriptor.byteCount <= limits.maximumOutputBytes else {
                throw AppPastedImageNormalizationError.invalidImage
            }
            try Task.checkCancellation()
            let output = try Data(contentsOf: destinationURL, options: .mappedIfSafe)
            guard output.count == descriptor.byteCount,
                  output.starts(with: [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) else {
                throw AppPastedImageNormalizationError.invalidImage
            }
            return output
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as MediaWorkerProcessClientError {
            if case let .requestRejected(code) = error {
                switch code {
                case "output_too_large":
                    throw AppPastedImageNormalizationError.outputTooLarge
                case "invalid_image", "unauthorized_path", "invalid_request":
                    throw AppPastedImageNormalizationError.invalidImage
                default:
                    break
                }
            }
            throw error
        }
    }

    static func normalizeToPNG(
        _ data: Data,
        limits: AppPastedImageNormalizationLimits = .attachment
    ) throws -> Data {
        try Task.checkCancellation()
        guard !data.isEmpty,
              data.count <= limits.maximumInputBytes,
              limits.maximumSourcePixelCount > 0,
              limits.maximumDecodePixelSize > 0,
              limits.maximumOutputBytes > 0,
              let source = CGImageSourceCreateWithData(data as CFData, [
                kCGImageSourceShouldCache: false,
              ] as CFDictionary),
              CGImageSourceGetCount(source) > 0,
              CGImageSourceGetStatus(source) == .statusComplete,
              CGImageSourceGetStatusAtIndex(source, 0) == .statusComplete,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil)
                as? [CFString: Any],
              let widthValue = properties[kCGImagePropertyPixelWidth] as? NSNumber,
              let heightValue = properties[kCGImagePropertyPixelHeight] as? NSNumber else {
            throw AppPastedImageNormalizationError.invalidImage
        }

        let width = widthValue.doubleValue
        let height = heightValue.doubleValue
        guard width.isFinite,
              height.isFinite,
              width > 0,
              height > 0,
              width * height <= Double(limits.maximumSourcePixelCount) else {
            throw AppPastedImageNormalizationError.imageTooLarge
        }

        try Task.checkCancellation()
        guard let decoded = CGImageSourceCreateThumbnailAtIndex(source, 0, [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: limits.maximumDecodePixelSize,
            kCGImageSourceShouldCacheImmediately: true,
        ] as CFDictionary) else {
            throw AppPastedImageNormalizationError.invalidImage
        }

        try Task.checkCancellation()
        let encoded = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            encoded,
            "public.png" as CFString,
            1,
            nil
        ) else {
            throw AppPastedImageNormalizationError.cannotEncode
        }
        CGImageDestinationAddImage(destination, decoded, nil)
        guard CGImageDestinationFinalize(destination), encoded.length > 0 else {
            throw AppPastedImageNormalizationError.cannotEncode
        }
        guard encoded.length <= limits.maximumOutputBytes else {
            throw AppPastedImageNormalizationError.outputTooLarge
        }
        try Task.checkCancellation()
        return Data(referencing: encoded)
    }
}

enum AppPastedImageNormalizationError: LocalizedError {
    case invalidImage
    case imageTooLarge
    case cannotEncode
    case outputTooLarge

    var errorDescription: String? {
        switch self {
        case .invalidImage:
            "图片已损坏、格式不受支持或超过 20 MB。"
        case .imageTooLarge:
            "图片像素尺寸过大，无法安全处理。"
        case .cannotEncode:
            "无法处理这张图片，请换一张图片重试。"
        case .outputTooLarge:
            "处理后的图片超过 20 MB 限制。"
        }
    }
}
