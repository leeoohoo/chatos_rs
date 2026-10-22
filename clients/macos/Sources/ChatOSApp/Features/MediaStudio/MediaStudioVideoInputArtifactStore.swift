import ChatOSCore
import Darwin
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Owns short-lived, app-controlled copies of inputs selected for a Media Studio video.
/// The source may be security scoped; provider request construction happens later and must
/// never depend on that scope remaining open or on a full in-memory Base64 copy.
struct MediaStudioInputArtifactStore: Sendable {
    static let maximumBytes = 20 * 1_024 * 1_024
    static let maximumSourcePixelCount = 64_000_000

    let root: URL

    init(root: URL? = nil) {
        self.root = root ?? FileManager.default.temporaryDirectory.appendingPathComponent(
            "chatos-media-inputs-\(UUID().uuidString.lowercased())",
            isDirectory: true
        )
    }

    func importImage(
        from sourceURL: URL,
        name: String? = nil,
        mimeType suppliedMIMEType: String? = nil
    ) async throws -> ImageGenerationInputImage {
        try await AppCancellableDetachedWork.run {
            let accessed = sourceURL.startAccessingSecurityScopedResource()
            defer { if accessed { sourceURL.stopAccessingSecurityScopedResource() } }

            let values = try sourceURL.resourceValues(
                forKeys: [.isRegularFileKey, .fileSizeKey, .contentTypeKey]
            )
            guard values.isRegularFile == true,
                  let byteCount = values.fileSize,
                  byteCount > 0,
                  byteCount <= Self.maximumBytes else {
                throw MediaStudioInputImageError.tooLarge
            }
            let mimeType = try Self.imageMIMEType(
                suppliedMIMEType ?? values.contentType?.preferredMIMEType
            )
            try Self.validateImage(sourceURL)
            let destination = try copy(
                sourceURL,
                byteCount: byteCount,
                fileExtension: UTType(mimeType: mimeType)?.preferredFilenameExtension
                    ?? sourceURL.pathExtension
            )
            return ImageGenerationInputImage(
                name: name ?? sourceURL.lastPathComponent,
                mimeType: mimeType,
                fileURL: destination
            )
        }
    }

    func importAudio(from sourceURL: URL) async throws -> VideoGenerationInputAudio {
        try await AppCancellableDetachedWork.run {
            let accessed = sourceURL.startAccessingSecurityScopedResource()
            defer { if accessed { sourceURL.stopAccessingSecurityScopedResource() } }

            let values = try sourceURL.resourceValues(
                forKeys: [.isRegularFileKey, .fileSizeKey, .contentTypeKey]
            )
            let supportedMIMETypes = [
                "audio/mpeg", "audio/mp3", "audio/wav", "audio/x-wav", "audio/vnd.wave",
                "audio/mp4", "audio/x-m4a", "audio/aac",
            ]
            guard values.isRegularFile == true,
                  let byteCount = values.fileSize,
                  byteCount > 0,
                  byteCount <= Self.maximumBytes,
                  let mimeType = values.contentType?.preferredMIMEType?.lowercased(),
                  supportedMIMETypes.contains(mimeType) else {
                throw MediaStudioReferenceAudioError.unsupportedOrTooLarge
            }
            let destination = try copy(
                sourceURL,
                byteCount: byteCount,
                fileExtension: values.contentType?.preferredFilenameExtension
                    ?? sourceURL.pathExtension
            )
            return VideoGenerationInputAudio(
                name: sourceURL.lastPathComponent,
                mimeType: mimeType,
                fileURL: destination
            )
        }
    }

    func importImage(
        data: Data,
        name: String,
        mimeType suppliedMIMEType: String
    ) async throws -> ImageGenerationInputImage {
        try await AppCancellableDetachedWork.run {
            guard !data.isEmpty, data.count <= Self.maximumBytes else {
                throw MediaStudioInputImageError.tooLarge
            }
            let mimeType = try Self.imageMIMEType(suppliedMIMEType)
            try Self.validateImage(data)
            let destination = try write(
                data,
                fileExtension: UTType(mimeType: mimeType)?.preferredFilenameExtension ?? "png"
            )
            return ImageGenerationInputImage(
                name: name,
                mimeType: mimeType,
                fileURL: destination
            )
        }
    }

    func removeIfManaged(_ url: URL?) {
        guard let url,
              url.standardizedFileURL.deletingLastPathComponent() == root.standardizedFileURL else {
            return
        }
        try? FileManager.default.removeItem(at: url)
    }

    func removeAll() {
        try? FileManager.default.removeItem(at: root)
    }

    private func copy(
        _ sourceURL: URL,
        byteCount: Int,
        fileExtension rawExtension: String
    ) throws -> URL {
        try Task.checkCancellation()
        try FileManager.default.createDirectory(
            at: root,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: root.path
        )
        let fileExtension = rawExtension.lowercased().filter { $0.isLetter || $0.isNumber }
        let destination = root.appendingPathComponent(
            UUID().uuidString.lowercased() + (fileExtension.isEmpty ? "" : ".\(fileExtension)")
        )
        try AppBoundedFileReader.copy(
            sourceURL,
            to: destination,
            expectedBytes: byteCount,
            maximumBytes: Self.maximumBytes
        )
        return destination
    }

    private func write(_ data: Data, fileExtension rawExtension: String) throws -> URL {
        try Task.checkCancellation()
        try prepareRoot()
        try MediaDiskCapacityPolicy.ensureCapacity(
            in: root,
            writeBytes: Int64(data.count)
        )
        let destination = destinationURL(fileExtension: rawExtension)
        let descriptor = destination.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC,
                mode_t(S_IRUSR | S_IWUSR)
            )
        }
        guard descriptor >= 0 else { throw CocoaError(.fileWriteUnknown) }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        var completed = false
        defer {
            try? handle.close()
            if !completed { try? FileManager.default.removeItem(at: destination) }
        }
        try handle.write(contentsOf: data)
        try handle.synchronize()
        try handle.close()
        completed = true
        return destination
    }

    private func prepareRoot() throws {
        try FileManager.default.createDirectory(
            at: root,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: root.path
        )
    }

    private func destinationURL(fileExtension rawExtension: String) -> URL {
        let fileExtension = rawExtension.lowercased().filter { $0.isLetter || $0.isNumber }
        return root.appendingPathComponent(
            UUID().uuidString.lowercased() + (fileExtension.isEmpty ? "" : ".\(fileExtension)")
        )
    }

    private static func imageMIMEType(_ value: String?) throws -> String {
        guard let value,
              let type = UTType(mimeType: value),
              type.conforms(to: .image),
              let normalized = type.preferredMIMEType,
              ["image/png", "image/jpeg", "image/webp"].contains(normalized.lowercased()) else {
            throw MediaStudioInputImageError.notAnImage
        }
        return normalized.lowercased()
    }

    private static func validateImage(_ url: URL) throws {
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw MediaStudioInputImageError.cannotDecode }
        defer { Darwin.close(descriptor) }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_size > 0,
              status.st_size <= Self.maximumBytes else {
            throw MediaStudioInputImageError.cannotDecode
        }
        let descriptorURL = URL(fileURLWithPath: "/dev/fd/\(descriptor)")
        guard let source = CGImageSourceCreateWithURL(descriptorURL as CFURL, [
            kCGImageSourceShouldCache: false,
        ] as CFDictionary),
        CGImageSourceGetCount(source) > 0,
        let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil)
            as? [CFString: Any],
        let width = (properties[kCGImagePropertyPixelWidth] as? NSNumber)?.doubleValue,
        let height = (properties[kCGImagePropertyPixelHeight] as? NSNumber)?.doubleValue,
        width.isFinite, height.isFinite,
        width > 0, height > 0,
        width * height <= Double(Self.maximumSourcePixelCount) else {
            throw MediaStudioInputImageError.cannotDecode
        }
    }

    private static func validateImage(_ data: Data) throws {
        guard let source = CGImageSourceCreateWithData(data as CFData, [
            kCGImageSourceShouldCache: false,
        ] as CFDictionary),
        CGImageSourceGetCount(source) > 0,
        let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil)
            as? [CFString: Any],
        let width = (properties[kCGImagePropertyPixelWidth] as? NSNumber)?.doubleValue,
        let height = (properties[kCGImagePropertyPixelHeight] as? NSNumber)?.doubleValue,
        width.isFinite, height.isFinite,
        width > 0, height > 0,
        width * height <= Double(Self.maximumSourcePixelCount) else {
            throw MediaStudioInputImageError.cannotDecode
        }
    }
}

enum MediaStudioReferenceAudioError: LocalizedError {
    case unsupportedOrTooLarge

    var errorDescription: String? {
        "请选择不超过 20 MB 的 MP3、WAV、M4A 或 AAC 音频文件。"
    }
}
