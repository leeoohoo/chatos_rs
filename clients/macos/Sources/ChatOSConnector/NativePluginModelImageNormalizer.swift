import AppKit
import ChatOSNetworking
import Foundation
import ImageIO

enum NativePluginModelImageNormalizer {
    private static let jpegCompressionQuality: CGFloat = 0.82
    static let maximumEncodedCharacters = 28 * 1_024 * 1_024
    static let maximumDecodedBytes = 20 * 1_024 * 1_024
    static let maximumSourcePixelCount = 64_000_000
    static let maximumDisplayPixelSize = 4_096
    static let maximumImageBlocks = 8
    private static let estimatedDecodedBytes = maximumDisplayPixelSize
        * maximumDisplayPixelSize * 4

    /// Normalize image blocks only after the plugin runtime has consumed the
    /// original result for its local visual session. This keeps picture-in-
    /// picture frames lossless while giving model gateways a broadly compatible
    /// baseline JPEG payload.
    static func normalizeForModel(
        _ result: NativeJSONValue,
        taskID: String? = nil,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared
    ) async throws -> NativeJSONValue {
        try Task.checkCancellation()
        guard containsPNGImage(result) else { return result }
        let component = taskID.map { String($0.prefix(128)) }
            ?? UUID().uuidString.lowercased()
        return try await heavyMediaGovernor.perform(context: .init(
            taskID: "plugin-image:\(component):normalize",
            kind: .imageDecode,
            estimatedDecodedBytes: estimatedDecodedBytes
        )) {
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
    }

    private static func containsPNGImage(_ result: NativeJSONValue) -> Bool {
        result.jsonObject?["content"]?.jsonArray?.contains { item in
            guard item.jsonObject?["type"]?.jsonString == "image" else { return false }
            let object = item.jsonObject
            return object?["mimeType"]?.jsonString == "image/png"
                || object?["mime_type"]?.jsonString == "image/png"
                || object?["mime"]?.jsonString == "image/png"
        } == true
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
