import ChatOSCore
import ChatOSMediaWorkerClient
import ChatOSMediaWorkerProtocol
import CryptoKit
import Foundation

enum NativeLocalAgentAttachmentModelInput {
    // Match the shared MCP runtime's image budget. The Local Agent transport can chunk
    // logical messages, but model inputs remain independently bounded.
    static let maximumModelBytes = 2 * 1_024 * 1_024
    private static let outputPixelSizes = [4_096, 3_072, 2_048, 1_536, 1_024, 768, 512]

    static func result(
        source: NativeLocalAgentVerifiedAttachmentFile,
        record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        imported: LocalAgentJSONValue? = nil,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared
    ) async throws -> LocalAgentJSONValue {
        try Task.checkCancellation()
        guard record.mediaType.lowercased().hasPrefix("image/") else {
            throw NativeLocalAgentAttachmentModelInputError.notImage
        }
        guard source.identity.byteCount == Int64(record.byteSize),
              record.byteSize > 0,
              record.byteSize <= UInt64(NativeLocalAgentAttachmentVault.maximumAttachmentBytes) else {
            throw NativeLocalAgentAttachmentModelInputError.invalidImage
        }

        let output = try await normalizedImage(
            source: source,
            ownerUserID: ownerUserID,
            mediaWorker: mediaWorker
        )
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
        let text = String(
            decoding: try JSONEncoder().encode(LocalAgentJSONValue.object(fields)),
            as: UTF8.self
        )
        var result = fields
        result["text"] = .string(text)
        result["_structured_result"] = .object(fields)
        result["_model_input"] = .array([.object([
            "type": .string("input_image"),
            "image_url": .string("data:\(output.mime);base64,\(output.bytes.base64EncodedString())"),
            "detail": .string("high"),
        ])])
        return .object(result)
    }

    private static func normalizedImage(
        source: NativeLocalAgentVerifiedAttachmentFile,
        ownerUserID: String,
        mediaWorker: any MediaWorkerImageNormalizing
    ) async throws -> (bytes: Data, mime: String) {
        let root = source.url.deletingLastPathComponent().appendingPathComponent(
            ".model-image-\(UUID().uuidString.lowercased())",
            isDirectory: true
        )
        do {
            try FileManager.default.createDirectory(
                at: root,
                withIntermediateDirectories: false,
                attributes: [.posixPermissions: 0o700]
            )
        } catch {
            throw NativeLocalAgentAttachmentModelInputError.processingUnavailable
        }
        defer { try? FileManager.default.removeItem(at: root) }

        for (index, maximumPixels) in outputPixelSizes.enumerated() {
            try Task.checkCancellation()
            let destination = root.appendingPathComponent("attempt-\(index).png", isDirectory: false)
            guard FileManager.default.createFile(
                atPath: destination.path,
                contents: nil,
                attributes: [.posixPermissions: 0o600]
            ) else {
                throw NativeLocalAgentAttachmentModelInputError.processingUnavailable
            }
            do {
                let descriptor = try await mediaWorker.normalizePNG(
                    sourceURL: source.url,
                    sourceIdentity: source.identity,
                    destinationURL: destination,
                    ownerUserID: ownerUserID,
                    maximumSourceBytes: NativeLocalAgentAttachmentVault.maximumAttachmentBytes,
                    maximumSourcePixelCount: NativePluginModelImageNormalizer.maximumSourcePixelCount,
                    maximumOutputPixelSize: maximumPixels,
                    maximumOutputBytes: maximumModelBytes
                )
                try Task.checkCancellation()
                let bytes = try Data(contentsOf: destination, options: [.mappedIfSafe])
                guard descriptor.path == destination.path,
                      descriptor.mimeType == "image/png",
                      descriptor.byteCount == bytes.count,
                      descriptor.byteCount <= maximumModelBytes,
                      descriptor.pixelWidth > 0,
                      descriptor.pixelHeight > 0,
                      max(descriptor.pixelWidth, descriptor.pixelHeight) <= maximumPixels,
                      sha256(bytes) == descriptor.sha256.lowercased() else {
                    throw NativeLocalAgentAttachmentModelInputError.invalidImage
                }
                return (bytes, descriptor.mimeType)
            } catch is CancellationError {
                throw CancellationError()
            } catch let error as MediaWorkerProcessClientError {
                if case .requestRejected(code: "output_too_large") = error,
                   index < outputPixelSizes.count - 1 {
                    continue
                }
                if case let .requestRejected(code) = error {
                    switch code {
                    case "invalid_image", "unauthorized_path", "invalid_request":
                        throw NativeLocalAgentAttachmentModelInputError.invalidImage
                    case "output_too_large":
                        throw NativeLocalAgentAttachmentModelInputError.imageTooLarge
                    default:
                        throw NativeLocalAgentAttachmentModelInputError.processingUnavailable
                    }
                }
                throw NativeLocalAgentAttachmentModelInputError.processingUnavailable
            } catch let error as NativeLocalAgentAttachmentModelInputError {
                throw error
            } catch {
                throw NativeLocalAgentAttachmentModelInputError.processingUnavailable
            }
        }
        throw NativeLocalAgentAttachmentModelInputError.imageTooLarge
    }

    private static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}

enum NativeLocalAgentAttachmentModelInputError: LocalizedError, Equatable {
    case notImage, invalidImage, imageTooLarge, processingUnavailable

    var errorDescription: String? {
        switch self {
        case .notImage:
            "This attachment is not an image. Use content for text or workspace in a Task for file processing."
        case .invalidImage:
            "The verified attachment could not be decoded as an image, or exceeds the safe pixel limit."
        case .imageTooLarge:
            "The image could not fit the model transport budget. Provide a smaller image or use an authorized image-processing capability."
        case .processingUnavailable:
            "The isolated image-processing worker is unavailable. Try the attachment again."
        }
    }
}
