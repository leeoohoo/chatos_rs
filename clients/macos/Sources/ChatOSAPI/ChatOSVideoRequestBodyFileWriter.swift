import ChatOSCore
import Darwin
import Foundation

/// Builds one JSON request body without materializing referenced media or their Base64
/// representations as contiguous in-memory values.
enum ChatOSVideoRequestBodyFileWriter {
    static let maximumSourceBytes = 47 * 1_024 * 1_024
    static let maximumImageSourceBytes = 20 * 1_024 * 1_024
    static let maximumAudioSourceBytes = 20 * 1_024 * 1_024
    static let maximumRequestBytes = 64 * 1_024 * 1_024
    private static let inputChunkBytes = 48 * 1_024

    struct Source {
        var placeholder: String
        var url: URL
        var mimeType: String
        var maximumBytes: Int
    }

    private struct OpenedSource {
        var handle: FileHandle
        var byteCount: Int
        var markerRange: Range<Data.Index>
        var dataURLPrefix: Data
        var encodedByteCount: Int
    }

    static func write(
        payload: [String: Any],
        placeholder: String,
        sourceURL: URL,
        mimeType: String,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil
    ) throws -> HTTPRequestBodyFile {
        try write(
            payload: payload,
            sources: [
                .init(
                    placeholder: placeholder,
                    url: sourceURL,
                    mimeType: mimeType,
                    maximumBytes: maximumSourceBytes
                ),
            ],
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        )
    }

    static func write(
        payload: [String: Any],
        sources: [Source],
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil
    ) throws -> HTTPRequestBodyFile {
        guard !sources.isEmpty,
              Set(sources.map(\.placeholder)).count == sources.count else {
            throw MediaGenerationClientError.mediaUploadFailed("引用媒体占位符无效。")
        }

        let encodedPayload = try JSONSerialization.data(withJSONObject: payload)
        var openedSources: [OpenedSource] = []
        openedSources.reserveCapacity(sources.count)
        do {
            for source in sources {
                let marker = Data("\"\(source.placeholder)\"".utf8)
                guard let markerRange = encodedPayload.range(of: marker),
                      encodedPayload[markerRange.upperBound...].range(of: marker) == nil else {
                    throw MediaGenerationClientError.mediaUploadFailed(
                        "引用媒体占位符没有唯一出现在请求中。"
                    )
                }
                let opened = try openRegularSource(
                    source.url,
                    maximumBytes: source.maximumBytes
                )
                let dataURLPrefix = Data(
                    "\"data:\(source.mimeType.lowercased());base64,".utf8
                )
                openedSources.append(.init(
                    handle: opened.handle,
                    byteCount: opened.byteCount,
                    markerRange: markerRange,
                    dataURLPrefix: dataURLPrefix,
                    encodedByteCount: 4 * ((opened.byteCount + 2) / 3)
                ))
            }
        } catch {
            for source in openedSources { try? source.handle.close() }
            throw error
        }
        defer { for source in openedSources { try? source.handle.close() } }
        openedSources.sort { $0.markerRange.lowerBound < $1.markerRange.lowerBound }

        var requestByteCount = encodedPayload.count
        for source in openedSources {
            let replacementBytes = source.dataURLPrefix.count
                + source.encodedByteCount
                + 1
            let markerBytes = source.markerRange.count
            let (withoutMarker, subtractOverflow) = requestByteCount.subtractingReportingOverflow(markerBytes)
            let (withReplacement, addOverflow) = withoutMarker.addingReportingOverflow(replacementBytes)
            guard !subtractOverflow, !addOverflow else {
                throw MediaGenerationClientError.mediaUploadFailed("媒体请求大小计算溢出。")
            }
            requestByteCount = withReplacement
        }
        guard requestByteCount > 0, requestByteCount <= maximumRequestBytes else {
            throw MediaGenerationClientError.mediaUploadFailed(
                "引用媒体编码后的请求不能超过 64 MiB。"
            )
        }

        do {
            if let availableCapacityOverride {
                try MediaDiskCapacityPolicy.ensureCapacity(
                    availableBytes: availableCapacityOverride,
                    writeBytes: Int64(requestByteCount)
                )
            } else {
                try MediaDiskCapacityPolicy.ensureCapacity(
                    in: temporaryDirectory,
                    writeBytes: Int64(requestByteCount)
                )
            }
        } catch let error as MediaDiskCapacityError {
            throw MediaGenerationClientError.mediaUploadFailed(
                error.localizedDescription
            )
        }

        let destination = try makeDestination(in: temporaryDirectory)
        var completed = false
        defer {
            try? destination.handle.close()
            if !completed { try? FileManager.default.removeItem(at: destination.url) }
        }

        do {
            var payloadCursor = encodedPayload.startIndex
            for source in openedSources {
                try destination.handle.write(
                    contentsOf: encodedPayload[payloadCursor..<source.markerRange.lowerBound]
                )
                try destination.handle.write(contentsOf: source.dataURLPrefix)
                var remainingSourceBytes = source.byteCount
                while remainingSourceBytes > 0 {
                    try Task.checkCancellation()
                    let chunk = try source.handle.read(
                        upToCount: min(inputChunkBytes, remainingSourceBytes)
                    ) ?? Data()
                    guard !chunk.isEmpty else {
                        throw MediaGenerationClientError.mediaUploadFailed(
                            "读取引用媒体时文件提前结束。"
                        )
                    }
                    remainingSourceBytes -= chunk.count
                    try destination.handle.write(contentsOf: chunk.base64EncodedData())
                }
                try destination.handle.write(contentsOf: Data("\"".utf8))
                payloadCursor = source.markerRange.upperBound
            }
            try destination.handle.write(contentsOf: encodedPayload[payloadCursor...])
            try destination.handle.synchronize()
            try destination.handle.close()
            completed = true
            return HTTPRequestBodyFile(
                url: destination.url,
                byteCount: requestByteCount,
                deleteWhenDone: true
            )
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as MediaGenerationClientError {
            throw error
        } catch {
            throw MediaGenerationClientError.mediaUploadFailed(error.localizedDescription)
        }
    }

    private static func openRegularSource(
        _ url: URL,
        maximumBytes: Int
    ) throws -> (handle: FileHandle, byteCount: Int) {
        guard url.isFileURL, maximumBytes > 0 else {
            throw MediaGenerationClientError.mediaUploadFailed("引用媒体文件无效。")
        }
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else {
            throw MediaGenerationClientError.mediaUploadFailed("无法打开引用媒体文件。")
        }

        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_size > 0,
              status.st_size <= maximumBytes else {
            Darwin.close(descriptor)
            throw MediaGenerationClientError.mediaUploadFailed(
                "引用媒体必须是大小受限的普通文件。"
            )
        }
        return (
            FileHandle(fileDescriptor: descriptor, closeOnDealloc: true),
            Int(status.st_size)
        )
    }

    private static func makeDestination(in temporaryDirectory: URL) throws -> (url: URL, handle: FileHandle) {
        let url = temporaryDirectory.appendingPathComponent(
            "chatos-video-request-\(UUID().uuidString.lowercased()).json",
            isDirectory: false
        )
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC,
                mode_t(S_IRUSR | S_IWUSR)
            )
        }
        guard descriptor >= 0 else {
            throw MediaGenerationClientError.mediaUploadFailed("无法创建临时请求文件。")
        }
        return (url, FileHandle(fileDescriptor: descriptor, closeOnDealloc: true))
    }
}
