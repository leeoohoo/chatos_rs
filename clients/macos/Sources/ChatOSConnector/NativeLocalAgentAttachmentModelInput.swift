import AppKit
import ChatOSCore
import Foundation
import ImageIO

enum NativeLocalAgentAttachmentModelInput {
    // Match the shared MCP runtime's image budget, below the 4 MiB IPC frame.
    static let maximumModelBytes = 2 * 1_024 * 1_024

    static func result(
        _ data: Data,
        record: LocalAgentConversationAttachmentRecord,
        imported: LocalAgentJSONValue? = nil
    ) async throws -> LocalAgentJSONValue {
        let task = Task<LocalAgentJSONValue, Error>.detached(priority: .userInitiated) {
            try Task.checkCancellation()
            guard record.mediaType.lowercased().hasPrefix("image/") else {
                throw NativeLocalAgentAttachmentModelInputError.notImage
            }
            let image = try modelImage(data)
            var fields: [String: LocalAgentJSONValue] = [:]
            if case let .object(value)? = imported { fields = value }
            fields["display_name"] = .string(record.displayName)
            fields["media_type"] = .string(record.mediaType)
            fields["byte_size"] = .number(Double(record.byteSize))
            fields["sha256"] = .string(record.sha256.lowercased())
            fields["sha256_verified"] = .bool(true)
            fields["delivery"] = imported == nil ? .string("model") : .string("workspace")
            fields["model_image_supplied"] = .bool(true)
            // Keep binary data out of the text tool result and structured memory.
            // The shared runtime extracts _model_input into transient image parts.
            let text = String(decoding: try JSONEncoder().encode(LocalAgentJSONValue.object(fields)), as: UTF8.self)
            var result = fields
            result["text"] = .string(text)
            result["_structured_result"] = .object(fields)
            result["_model_input"] = .array([.object([
                    "type": .string("input_image"),
                    "image_url": .string("data:\(image.mime);base64,\(image.bytes.base64EncodedString())"),
                    "detail": .string("high"),
                ])])
            return .object(result)
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }

    private static func modelImage(_ data: Data) throws -> (bytes: Data, mime: String) {
        guard !data.isEmpty, data.count <= NativeLocalAgentAttachmentVault.maximumAttachmentBytes,
              let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = (properties[kCGImagePropertyPixelWidth] as? NSNumber)?.doubleValue,
              let height = (properties[kCGImagePropertyPixelHeight] as? NSNumber)?.doubleValue,
              width > 0, height > 0, width.isFinite, height.isFinite,
              width * height <= Double(NativePluginModelImageNormalizer.maximumSourcePixelCount) else {
            throw NativeLocalAgentAttachmentModelInputError.invalidImage
        }
        // Decode even small inputs; a header or a MIME label alone is not a valid image.
        let orientation = (properties[kCGImagePropertyOrientation] as? NSNumber)?.intValue ?? 1
        let type = CGImageSourceGetType(source) as String?
        let originalMime = ["public.png": "image/png", "public.jpeg": "image/jpeg", "org.webmproject.webp": "image/webp"][type ?? ""]
        for maximumPixels in [4_096, 3_072, 2_048, 1_536] {
            try Task.checkCancellation()
            guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maximumPixels,
                kCGImageSourceShouldCacheImmediately: true,
            ] as CFDictionary) else { throw NativeLocalAgentAttachmentModelInputError.invalidImage }
            if maximumPixels == 4_096, data.count <= maximumModelBytes,
               max(width, height) <= 4_096, orientation == 1,
               CGImageSourceGetCount(source) == 1, let originalMime {
                return (data, originalMime)
            }
            for quality in [0.88, 0.72] {
                if let jpeg = NSBitmapImageRep(cgImage: image).representation(
                    using: .jpeg, properties: [.compressionFactor: quality]
                ), jpeg.count <= maximumModelBytes {
                    return (jpeg, "image/jpeg")
                }
            }
        }
        throw NativeLocalAgentAttachmentModelInputError.imageTooLarge
    }
}

enum NativeLocalAgentAttachmentModelInputError: LocalizedError, Equatable {
    case notImage, invalidImage, imageTooLarge

    var errorDescription: String? {
        switch self {
        case .notImage: "This attachment is not an image. Use content for text or workspace in a Task for file processing."
        case .invalidImage: "The verified attachment could not be decoded as an image, or exceeds the safe pixel limit."
        case .imageTooLarge: "The image could not fit the model transport budget. Provide a smaller image or use an authorized image-processing capability."
        }
    }
}
