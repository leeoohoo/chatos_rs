import AppKit
import Foundation
import ImageIO

enum NativePluginModelImageNormalizer {
    private static let jpegCompressionQuality: CGFloat = 0.82
    static let maximumEncodedCharacters = 28 * 1_024 * 1_024
    static let maximumDecodedBytes = 20 * 1_024 * 1_024
    static let maximumSourcePixelCount = 64_000_000
    static let maximumDisplayPixelSize = 4_096
    static let maximumImageBlocks = 8

    /// Normalize image blocks only after the plugin runtime has consumed the
    /// original result for its local visual session. This keeps picture-in-
    /// picture frames lossless while giving model gateways a broadly compatible
    /// baseline JPEG payload.
    static func normalizeForModel(_ result: NativeJSONValue) async throws -> NativeJSONValue {
        let task = Task.detached(priority: .userInitiated) {
            try Task.checkCancellation()
            return try normalize(result)
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }

    nonisolated private static func normalize(_ result: NativeJSONValue) throws -> NativeJSONValue {
        guard case var .object(root) = result,
              case let .array(content)? = root["content"] else {
            return result
        }

        var normalized: [NativeJSONValue] = []
        normalized.reserveCapacity(content.count)
        var imageBlockCount = 0
        for item in content {
            try Task.checkCancellation()
            let isImage = item.jsonObject?["type"]?.jsonString == "image"
            if isImage {
                imageBlockCount += 1
            }
            normalized.append(
                imageBlockCount > maximumImageBlocks && isImage
                    ? unavailableItem("the plugin returned too many image blocks")
                    : normalizeContentItem(item)
            )
        }
        root["content"] = .array(normalized)
        return .object(root)
    }

    nonisolated private static func normalizeContentItem(
        _ item: NativeJSONValue
    ) -> NativeJSONValue {
        guard case var .object(object) = item,
              object["type"]?.jsonString == "image" else {
            return item
        }
        let mimeType = object["mimeType"]?.jsonString
            ?? object["mime_type"]?.jsonString
            ?? object["mime"]?.jsonString
        guard mimeType == "image/png" else {
            return item
        }
        guard let encoded = object["data"]?.jsonString,
              encoded.utf8.count <= maximumEncodedCharacters,
              let pngData = Data(base64Encoded: encoded),
              !pngData.isEmpty,
              pngData.count <= maximumDecodedBytes,
              let jpegData = jpegData(fromPNGData: pngData) else {
            return unavailableItem("the plugin returned PNG data that could not be decoded locally")
        }

        object["data"] = .string(jpegData.base64EncodedString())
        object["mimeType"] = .string("image/jpeg")
        object.removeValue(forKey: "mime_type")
        object.removeValue(forKey: "mime")
        return .object(object)
    }

    nonisolated private static func jpegData(fromPNGData data: Data) -> Data? {
        guard !Task.isCancelled,
              let source = CGImageSourceCreateWithData(data as CFData, [
                kCGImageSourceShouldCache: false,
              ] as CFDictionary),
              CGImageSourceGetCount(source) > 0,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil)
                as? [CFString: Any],
              let widthValue = properties[kCGImagePropertyPixelWidth] as? NSNumber,
              let heightValue = properties[kCGImagePropertyPixelHeight] as? NSNumber else {
            return nil
        }
        let width = widthValue.doubleValue
        let height = heightValue.doubleValue
        guard !Task.isCancelled,
              width.isFinite, height.isFinite,
              width > 0, height > 0,
              width * height <= Double(maximumSourcePixelCount),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maximumDisplayPixelSize,
                kCGImageSourceShouldCacheImmediately: true,
              ] as CFDictionary),
              !Task.isCancelled,
              let jpeg = NSBitmapImageRep(cgImage: image).representation(
            using: .jpeg,
            properties: [.compressionFactor: jpegCompressionQuality]
              ),
              jpeg.count <= maximumDecodedBytes else {
            return nil
        }
        return jpeg
    }

    nonisolated private static func unavailableItem(_ reason: String) -> NativeJSONValue {
        .object([
            "type": .string("text"),
            "text": .string(
                "[Screenshot unavailable: \(reason). Refresh the application state before continuing.]"
            ),
        ])
    }
}
