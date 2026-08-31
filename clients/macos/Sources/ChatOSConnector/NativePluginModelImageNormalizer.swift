import AppKit
import Foundation
import ImageIO

enum NativePluginModelImageNormalizer {
    private static let jpegCompressionQuality: CGFloat = 0.82

    /// Normalize image blocks only after the plugin runtime has consumed the
    /// original result for its local visual session. This keeps picture-in-
    /// picture frames lossless while giving model gateways a broadly compatible
    /// baseline JPEG payload.
    static func normalizeForModel(_ result: NativeJSONValue) -> NativeJSONValue {
        guard case var .object(root) = result,
              case let .array(content)? = root["content"] else {
            return result
        }

        root["content"] = .array(content.map(normalizeContentItem))
        return .object(root)
    }

    private static func normalizeContentItem(_ item: NativeJSONValue) -> NativeJSONValue {
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
              let pngData = Data(base64Encoded: encoded),
              let jpegData = jpegData(fromPNGData: pngData) else {
            return .object([
                "type": .string("text"),
                "text": .string("[Screenshot unavailable: the plugin returned PNG data that could not be decoded locally. Refresh the application state before continuing.]"),
            ])
        }

        object["data"] = .string(jpegData.base64EncodedString())
        object["mimeType"] = .string("image/jpeg")
        object.removeValue(forKey: "mime_type")
        object.removeValue(forKey: "mime")
        return .object(object)
    }

    private static func jpegData(fromPNGData data: Data) -> Data? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            return nil
        }
        return NSBitmapImageRep(cgImage: image).representation(
            using: .jpeg,
            properties: [.compressionFactor: jpegCompressionQuality]
        )
    }
}
