import ChatOSAPI
import ChatOSCore
import Foundation
import ImageIO
import UniformTypeIdentifiers

enum MediaStudioInputImagePolicy {
    static let maximumBytes = 20 * 1_024 * 1_024
    static let maximumSourcePixelCount = 64_000_000
}

struct MediaStudioImageReferenceImporter: MediaStudioImageReferenceImporting {
    private let transport: any HTTPTransport

    init(transport: any HTTPTransport = URLSessionHTTPTransport()) {
        self.transport = transport
    }

    func importLocalFiles(_ urls: [URL]) async throws -> [ImageGenerationInputImage] {
        try await AppCancellableDetachedWork.run {
            try urls.map { try Self.loadInputImage(from: $0) }
        }
    }

    func importGeneratedAssets(
        _ assets: [GeneratedMediaAsset]
    ) async throws -> [ImageGenerationInputImage] {
        var imported: [ImageGenerationInputImage] = []
        imported.reserveCapacity(assets.count)
        for asset in assets {
            try Task.checkCancellation()
            let data = try await MediaStudioImageLoader.data(for: asset, transport: transport)
            let input = try await AppCancellableDetachedWork.run {
                try Self.makeInputImage(
                    data: data,
                    name: "generated-\(asset.id).\(Self.fileExtension(for: asset.mimeType))",
                    mimeType: asset.mimeType
                )
            }
            imported.append(input)
        }
        return imported
    }

    private static func loadInputImage(from url: URL) throws -> ImageGenerationInputImage {
        try Task.checkCancellation()
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }

        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey, .contentTypeKey])
        guard values.isRegularFile == true,
              values.contentType?.conforms(to: .image) == true else {
            throw MediaStudioInputImageError.notAnImage
        }
        if let fileSize = values.fileSize,
           fileSize > MediaStudioInputImagePolicy.maximumBytes {
            throw MediaStudioInputImageError.tooLarge
        }

        let sourceData = try AppBoundedFileReader.read(
            url,
            maximumBytes: MediaStudioInputImagePolicy.maximumBytes
        )
        try Task.checkCancellation()
        return try makeInputImage(
            data: sourceData,
            name: url.lastPathComponent,
            mimeType: values.contentType?.preferredMIMEType
        )
    }

    private static func makeInputImage(
        data: Data,
        name: String,
        mimeType: String? = nil
    ) throws -> ImageGenerationInputImage {
        try Task.checkCancellation()
        guard !data.isEmpty,
              data.count <= MediaStudioInputImagePolicy.maximumBytes else {
            throw MediaStudioInputImageError.tooLarge
        }
        guard let source = CGImageSourceCreateWithData(data as CFData, [
                kCGImageSourceShouldCache: false,
              ] as CFDictionary),
              CGImageSourceGetCount(source) > 0,
              CGImageSourceGetStatus(source) == .statusComplete,
              CGImageSourceGetStatusAtIndex(source, 0) == .statusComplete,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil)
                as? [CFString: Any],
              let widthValue = properties[kCGImagePropertyPixelWidth] as? NSNumber,
              let heightValue = properties[kCGImagePropertyPixelHeight] as? NSNumber else {
            throw MediaStudioInputImageError.cannotDecode
        }
        let width = widthValue.doubleValue
        let height = heightValue.doubleValue
        guard width.isFinite, height.isFinite,
              width > 0, height > 0,
              width * height <= Double(MediaStudioInputImagePolicy.maximumSourcePixelCount) else {
            throw MediaStudioInputImageError.cannotDecode
        }
        try Task.checkCancellation()
        let base64Data = data.base64EncodedString()
        try Task.checkCancellation()
        return ImageGenerationInputImage(
            name: name,
            mimeType: normalizedImageMIMEType(mimeType),
            base64Data: base64Data
        )
    }

    private static func normalizedImageMIMEType(_ value: String?) -> String {
        guard let value, let type = UTType(mimeType: value), type.conforms(to: .image) else {
            return "image/png"
        }
        return type.preferredMIMEType ?? value
    }

    private static func fileExtension(for mimeType: String) -> String {
        UTType(mimeType: mimeType)?.preferredFilenameExtension ?? "png"
    }
}

enum MediaStudioInputImageError: LocalizedError {
    case notAnImage
    case cannotDecode
    case tooLarge

    var errorDescription: String? {
        switch self {
        case .notAnImage:
            "请选择有效的图片文件。"
        case .cannotDecode:
            "无法读取这张图片，请换一张后重试。"
        case .tooLarge:
            "参考图不能超过 20 MB。"
        }
    }
}
