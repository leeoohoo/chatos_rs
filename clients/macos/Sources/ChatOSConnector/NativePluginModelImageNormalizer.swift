import ChatOSMediaWorkerClient
import Foundation

enum NativePluginModelImageNormalizer {
    private static let jpegCompressionQuality = 0.82
    static let maximumEncodedCharacters = 28 * 1_024 * 1_024
    static let maximumDecodedBytes = 20 * 1_024 * 1_024
    static let maximumSourcePixelCount = 64_000_000
    static let maximumDisplayPixelSize = 4_096
    static let maximumImageBlocks = 8

    /// Normalize image blocks only after the plugin runtime has consumed the
    /// original result for its local visual session. Every eligible image is
    /// handled by its own short-lived media worker process so unrelated plugin
    /// calls and image blocks cannot serialize or crash the application process.
    static func normalizeForModel(
        _ result: NativeJSONValue,
        ownerUserID: String = "plugin-model-image",
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        mediaWorker: any MediaWorkerJPEGNormalizing = MediaWorkerProcessClient.shared
    ) async throws -> NativeJSONValue {
        try Task.checkCancellation()
        guard containsPNGImage(result),
              case var .object(root) = result,
              case let .array(content)? = root["content"] else {
            return result
        }

        var normalized = content
        var imageBlockCount = 0
        var work: [(index: Int, item: NativeJSONValue)] = []
        for (index, item) in content.enumerated() {
            let isImage = item.jsonObject?["type"]?.jsonString == "image"
            if isImage { imageBlockCount += 1 }
            if imageBlockCount > maximumImageBlocks, isImage {
                normalized[index] = unavailableItem("the plugin returned too many image blocks")
            } else if isPNGImage(item) {
                work.append((index, item))
            }
        }

        try await withThrowingTaskGroup(
            of: (Int, NativeJSONValue).self
        ) { group in
            for entry in work {
                group.addTask {
                    let item = try await normalizeContentItem(
                        entry.item,
                        index: entry.index,
                        ownerUserID: ownerUserID,
                        temporaryDirectory: temporaryDirectory,
                        mediaWorker: mediaWorker
                    )
                    return (entry.index, item)
                }
            }
            for try await (index, item) in group {
                normalized[index] = item
            }
        }

        root["content"] = .array(normalized)
        return .object(root)
    }

    private static func containsPNGImage(_ result: NativeJSONValue) -> Bool {
        result.jsonObject?["content"]?.jsonArray?.contains(where: isPNGImage) == true
    }

    private static func isPNGImage(_ item: NativeJSONValue) -> Bool {
        guard item.jsonObject?["type"]?.jsonString == "image" else { return false }
        let object = item.jsonObject
        return object?["mimeType"]?.jsonString == "image/png"
            || object?["mime_type"]?.jsonString == "image/png"
            || object?["mime"]?.jsonString == "image/png"
    }

    private static func normalizeContentItem(
        _ item: NativeJSONValue,
        index: Int,
        ownerUserID: String,
        temporaryDirectory: URL,
        mediaWorker: any MediaWorkerJPEGNormalizing
    ) async throws -> NativeJSONValue {
        guard case var .object(object) = item,
              let encoded = object["data"]?.jsonString,
              encoded.utf8.count <= maximumEncodedCharacters,
              let pngData = Data(base64Encoded: encoded),
              !pngData.isEmpty,
              pngData.count <= maximumDecodedBytes,
              !ownerUserID.isEmpty else {
            return unavailableItem(
                "the plugin returned PNG data that could not be decoded locally"
            )
        }

        let root = temporaryDirectory.appendingPathComponent(
            "ChatOSPluginImage-\(index)-\(UUID().uuidString.lowercased())",
            isDirectory: true
        )
        let sourceURL = root.appendingPathComponent("source.png", isDirectory: false)
        let destinationURL = root.appendingPathComponent("normalized.jpg", isDirectory: false)
        do {
            try FileManager.default.createDirectory(
                at: root,
                withIntermediateDirectories: false,
                attributes: [.posixPermissions: 0o700]
            )
            try pngData.write(to: sourceURL, options: .withoutOverwriting)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: sourceURL.path
            )
            guard FileManager.default.createFile(
                atPath: destinationURL.path,
                contents: nil,
                attributes: [.posixPermissions: 0o600]
            ) else {
                throw CocoaError(.fileWriteUnknown)
            }
        } catch {
            try? FileManager.default.removeItem(at: root)
            return unavailableItem("the plugin image could not be prepared for local decoding")
        }
        defer { try? FileManager.default.removeItem(at: root) }

        do {
            let descriptor = try await mediaWorker.normalizeJPEG(
                sourceURL: sourceURL,
                sourceIdentity: nil,
                destinationURL: destinationURL,
                ownerUserID: ownerUserID,
                maximumSourceBytes: maximumDecodedBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumOutputPixelSize: maximumDisplayPixelSize,
                maximumOutputBytes: maximumDecodedBytes,
                compressionQuality: jpegCompressionQuality
            )
            guard descriptor.path == destinationURL.path,
                  descriptor.mimeType == "image/jpeg",
                  descriptor.byteCount > 0,
                  descriptor.byteCount <= maximumDecodedBytes else {
                return unavailableItem(
                    "the plugin returned PNG data that could not be decoded locally"
                )
            }
            try Task.checkCancellation()
            let jpegData = try Data(contentsOf: destinationURL, options: .mappedIfSafe)
            guard jpegData.count == descriptor.byteCount,
                  jpegData.starts(with: [0xFF, 0xD8]),
                  jpegData.suffix(2) == Data([0xFF, 0xD9]) else {
                return unavailableItem(
                    "the plugin returned PNG data that could not be decoded locally"
                )
            }
            object["data"] = .string(jpegData.base64EncodedString())
            object["mimeType"] = .string("image/jpeg")
            object.removeValue(forKey: "mime_type")
            object.removeValue(forKey: "mime")
            return .object(object)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            return unavailableItem(
                "the plugin returned PNG data that could not be decoded locally"
            )
        }
    }

    private static func unavailableItem(_ reason: String) -> NativeJSONValue {
        .object([
            "type": .string("text"),
            "text": .string(
                "[Screenshot unavailable: \(reason). Refresh the application state before continuing.]"
            ),
        ])
    }
}
