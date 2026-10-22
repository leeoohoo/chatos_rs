import Darwin
import Foundation

enum AgentAttachmentDataLoader {
    static let maximumBytes = 20 * 1_024 * 1_024
    static let maximumTextPreviewBytes = 512 * 1_024

    struct TextPreview: Sendable, Equatable {
        let text: String
        let isTruncated: Bool
    }

    static func load(_ url: URL) throws -> Data {
        let source = try open(url)
        defer { Darwin.close(source.descriptor) }
        let bytes = try readPrefix(
            descriptor: source.descriptor,
            count: source.byteCount
        )
        try verifyUnchanged(source)
        return bytes
    }

    static func textPreview(_ url: URL) throws -> TextPreview? {
        let source = try open(url)
        defer { Darwin.close(source.descriptor) }
        let previewByteCount = min(source.byteCount, maximumTextPreviewBytes)
        let bytes = try readPrefix(
            descriptor: source.descriptor,
            count: previewByteCount
        )
        try verifyUnchanged(source)
        guard !bytes.prefix(8_000).contains(0),
              let text = String(data: bytes, encoding: .utf8) else {
            return nil
        }
        return .init(
            text: text,
            isTruncated: source.byteCount > maximumTextPreviewBytes
        )
    }

    private struct OpenedSource {
        let descriptor: Int32
        let byteCount: Int
        let status: stat
    }

    private static func open(_ url: URL) throws -> OpenedSource {
        guard url.isFileURL else { throw AgentRuntimeError.invalidAttachment }
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw AgentRuntimeError.invalidAttachment }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_nlink == 1,
              status.st_size > 0,
              status.st_size <= maximumBytes else {
            Darwin.close(descriptor)
            throw AgentRuntimeError.invalidAttachment
        }
        return .init(
            descriptor: descriptor,
            byteCount: Int(status.st_size),
            status: status
        )
    }

    private static func readPrefix(descriptor: Int32, count: Int) throws -> Data {
        var bytes = Data(count: count)
        var offset = 0
        while offset < count {
            try Task.checkCancellation()
            let readCount = bytes.withUnsafeMutableBytes { buffer in
                Darwin.read(
                    descriptor,
                    buffer.baseAddress?.advanced(by: offset),
                    count - offset
                )
            }
            if readCount < 0, errno == EINTR { continue }
            guard readCount > 0 else { throw AgentRuntimeError.invalidAttachment }
            offset += readCount
        }
        return bytes
    }

    private static func verifyUnchanged(_ source: OpenedSource) throws {
        var finalStatus = stat()
        guard fstat(source.descriptor, &finalStatus) == 0,
              source.status.st_dev == finalStatus.st_dev,
              source.status.st_ino == finalStatus.st_ino,
              source.status.st_nlink == finalStatus.st_nlink,
              source.status.st_size == finalStatus.st_size,
              source.status.st_mtimespec.tv_sec == finalStatus.st_mtimespec.tv_sec,
              source.status.st_mtimespec.tv_nsec == finalStatus.st_mtimespec.tv_nsec,
              source.status.st_ctimespec.tv_sec == finalStatus.st_ctimespec.tv_sec,
              source.status.st_ctimespec.tv_nsec == finalStatus.st_ctimespec.tv_nsec else {
            throw AgentRuntimeError.invalidAttachment
        }
    }
}
