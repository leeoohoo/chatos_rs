import ChatOSCore
import Darwin
import Foundation

/// Builds an image-edit multipart body from local files without collecting the selected
/// images or the finished multipart body in memory.
enum ChatOSImageMultipartBodyFileWriter {
    static let maximumImageBytes = 20 * 1_024 * 1_024
    static let maximumImageCount = 8
    static let maximumRequestBytes = 161 * 1_024 * 1_024
    private static let chunkBytes = 256 * 1_024

    private struct OpenedImage {
        var handle: FileHandle
        var byteCount: Int
        var header: Data
    }

    static func write(
        runtime: RuntimeModelConfig,
        request: ImageGenerationRequest,
        images: [ImageGenerationInputImage],
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil
    ) throws -> (boundary: String, bodyFile: HTTPRequestBodyFile) {
        guard !images.isEmpty, images.count <= maximumImageCount else {
            throw MediaGenerationClientError.invalidInputImage
        }
        let boundary = "ChatOSMediaBoundary\(UUID().uuidString.replacingOccurrences(of: "-", with: ""))"
        var prelude = Data()
        ChatOSMediaGenerationService.appendMultipartField(
            name: "model", value: runtime.model, boundary: boundary, to: &prelude
        )
        ChatOSMediaGenerationService.appendMultipartField(
            name: "prompt", value: request.prompt, boundary: boundary, to: &prelude
        )
        ChatOSMediaGenerationService.appendMultipartField(
            name: "n", value: String(request.count), boundary: boundary, to: &prelude
        )
        if let size = request.size {
            ChatOSMediaGenerationService.appendMultipartField(
                name: "size", value: size, boundary: boundary, to: &prelude
            )
        }

        var opened: [OpenedImage] = []
        opened.reserveCapacity(images.count)
        do {
            for image in images {
                guard image.base64Data.isEmpty,
                      let fileURL = image.fileURL,
                      fileURL.isFileURL,
                      ["image/png", "image/jpeg", "image/webp"]
                        .contains(image.mimeType.lowercased()) else {
                    throw MediaGenerationClientError.invalidInputImage
                }
                let source = try openRegularFile(fileURL)
                let field = images.count > 1 ? "image[]" : "image"
                let fileName = ChatOSMediaGenerationService.sanitizedFileName(
                    image.name,
                    mimeType: image.mimeType.lowercased()
                )
                let header = Data(
                    "--\(boundary)\r\nContent-Disposition: form-data; name=\"\(field)\"; filename=\"\(fileName)\"\r\nContent-Type: \(image.mimeType.lowercased())\r\n\r\n".utf8
                )
                opened.append(.init(
                    handle: source.handle,
                    byteCount: source.byteCount,
                    header: header
                ))
            }
        } catch {
            for image in opened { try? image.handle.close() }
            throw error
        }
        defer { for image in opened { try? image.handle.close() } }

        let footer = Data("--\(boundary)--\r\n".utf8)
        var requestBytes = prelude.count + footer.count
        for image in opened {
            let (withHeader, headerOverflow) = requestBytes.addingReportingOverflow(image.header.count)
            let (withImage, imageOverflow) = withHeader.addingReportingOverflow(image.byteCount)
            let (withTerminator, terminatorOverflow) = withImage.addingReportingOverflow(2)
            guard !headerOverflow, !imageOverflow, !terminatorOverflow else {
                throw MediaGenerationClientError.mediaUploadFailed("图片请求大小计算溢出。")
            }
            requestBytes = withTerminator
        }
        guard requestBytes > 0, requestBytes <= maximumRequestBytes else {
            throw MediaGenerationClientError.mediaUploadFailed("图片请求不能超过 161 MiB。")
        }

        ChatOSUploadBodyArtifact.performMaintenance(in: temporaryDirectory)
        do {
            if let availableCapacityOverride {
                try MediaDiskCapacityPolicy.ensureCapacity(
                    availableBytes: availableCapacityOverride,
                    writeBytes: Int64(requestBytes)
                )
            } else {
                try MediaDiskCapacityPolicy.ensureCapacity(
                    in: temporaryDirectory,
                    writeBytes: Int64(requestBytes)
                )
            }
        } catch let error as MediaDiskCapacityError {
            throw MediaGenerationClientError.mediaUploadFailed(error.localizedDescription)
        }

        let destination = try makeDestination(in: temporaryDirectory)
        var completed = false
        defer {
            try? destination.handle.close()
            if !completed { destination.owner.removeArtifact() }
        }
        do {
            try destination.handle.write(contentsOf: prelude)
            for image in opened {
                try Task.checkCancellation()
                try destination.handle.write(contentsOf: image.header)
                var remaining = image.byteCount
                while remaining > 0 {
                    try Task.checkCancellation()
                    let chunk = try image.handle.read(upToCount: min(chunkBytes, remaining)) ?? Data()
                    guard !chunk.isEmpty else {
                        throw MediaGenerationClientError.mediaUploadFailed(
                            "读取参考图片时文件提前结束。"
                        )
                    }
                    remaining -= chunk.count
                    try destination.handle.write(contentsOf: chunk)
                }
                try destination.handle.write(contentsOf: Data("\r\n".utf8))
            }
            try destination.handle.write(contentsOf: footer)
            try destination.handle.synchronize()
            try destination.handle.close()
            completed = true
            let owner = destination.owner
            return (
                boundary,
                HTTPRequestBodyFile(
                    url: owner.payloadURL,
                    byteCount: requestBytes,
                    deleteWhenDone: true,
                    cleanup: { owner.removeArtifact() }
                )
            )
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as MediaGenerationClientError {
            throw error
        } catch {
            throw MediaGenerationClientError.mediaUploadFailed(error.localizedDescription)
        }
    }

    private static func openRegularFile(_ url: URL) throws -> (handle: FileHandle, byteCount: Int) {
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw MediaGenerationClientError.invalidInputImage }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_size > 0,
              status.st_size <= maximumImageBytes else {
            Darwin.close(descriptor)
            throw MediaGenerationClientError.invalidInputImage
        }
        return (
            FileHandle(fileDescriptor: descriptor, closeOnDealloc: true),
            Int(status.st_size)
        )
    }

    private static func makeDestination(
        in directory: URL
    ) throws -> ChatOSUploadBodyArtifact.Destination {
        do {
            return try ChatOSUploadBodyArtifact.makeDestination(
                kind: .imageMultipart,
                in: directory
            )
        } catch {
            throw MediaGenerationClientError.mediaUploadFailed("无法创建图片请求临时文件。")
        }
    }
}
