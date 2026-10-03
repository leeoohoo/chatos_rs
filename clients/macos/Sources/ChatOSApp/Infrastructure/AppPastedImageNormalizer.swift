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
        limits: AppPastedImageNormalizationLimits = .attachment
    ) async throws -> Data {
        let task = Task.detached(priority: .userInitiated) {
            try normalizeToPNG(data, limits: limits)
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
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
