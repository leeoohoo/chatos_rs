import CryptoKit
import Darwin
import Foundation

enum ConversationAttachmentImageLoader {
    static let maximumBytes = 20 * 1_024 * 1_024
    static let maximumSourcePixelCount = 64_000_000

    static func load(
        url: URL,
        sha256: String?,
        expectedBytes: Int,
        maximumDisplayPixelSize: Int = 2_048
    ) async throws -> AppDecodedThumbnail {
        try await AppCancellableDetachedWork.run {
            try loadSynchronously(
                url: url,
                sha256: sha256,
                expectedBytes: expectedBytes,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
        }
    }

    private nonisolated static func loadSynchronously(
        url: URL,
        sha256: String?,
        expectedBytes: Int,
        maximumDisplayPixelSize: Int
    ) throws -> AppDecodedThumbnail {
        guard url.isFileURL,
              expectedBytes > 0,
              expectedBytes <= maximumBytes,
              maximumDisplayPixelSize > 0,
              let sha256,
              sha256.utf8.count == 64 else {
            throw CocoaError(.fileReadCorruptFile)
        }
        try Task.checkCancellation()
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CocoaError(.fileReadNoSuchFile) }
        defer { Darwin.close(descriptor) }

        var initialStatus = stat()
        guard fstat(descriptor, &initialStatus) == 0,
              (initialStatus.st_mode & S_IFMT) == S_IFREG,
              initialStatus.st_size == expectedBytes else {
            throw CocoaError(.fileReadCorruptFile)
        }
        guard try digest(descriptor: descriptor, expectedBytes: expectedBytes) == sha256.lowercased() else {
            throw CocoaError(.fileReadCorruptFile)
        }
        try Task.checkCancellation()
        guard let decoded = AppImageThumbnailLoader.decodeOpenFileDescriptor(
            descriptor,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumDisplayPixelSize: maximumDisplayPixelSize
        ) else {
            try Task.checkCancellation()
            throw CocoaError(.fileReadCorruptFile)
        }
        try Task.checkCancellation()
        var finalStatus = stat()
        guard fstat(descriptor, &finalStatus) == 0,
              finalStatus.st_dev == initialStatus.st_dev,
              finalStatus.st_ino == initialStatus.st_ino,
              finalStatus.st_size == initialStatus.st_size,
              finalStatus.st_mtimespec.tv_sec == initialStatus.st_mtimespec.tv_sec,
              finalStatus.st_mtimespec.tv_nsec == initialStatus.st_mtimespec.tv_nsec else {
            throw CocoaError(.fileReadCorruptFile)
        }
        return decoded
    }

    private nonisolated static func digest(
        descriptor: Int32,
        expectedBytes: Int
    ) throws -> String {
        guard lseek(descriptor, 0, SEEK_SET) == 0 else {
            throw CocoaError(.fileReadCorruptFile)
        }
        var hasher = SHA256()
        var buffer = [UInt8](repeating: 0, count: 64 * 1_024)
        var totalBytes = 0
        while totalBytes < expectedBytes {
            try Task.checkCancellation()
            let requested = min(buffer.count, expectedBytes - totalBytes)
            let count = buffer.withUnsafeMutableBytes { bytes in
                Darwin.read(descriptor, bytes.baseAddress, requested)
            }
            if count > 0 {
                buffer.withUnsafeBytes { bytes in
                    hasher.update(bufferPointer: UnsafeRawBufferPointer(
                        rebasing: bytes.prefix(count)
                    ))
                }
                totalBytes += count
            } else if count < 0, errno == EINTR {
                continue
            } else {
                throw CocoaError(.fileReadCorruptFile)
            }
        }
        var trailingByte: UInt8 = 0
        while true {
            let trailingCount = Darwin.read(descriptor, &trailingByte, 1)
            if trailingCount == 0 { break }
            if trailingCount < 0, errno == EINTR { continue }
            throw CocoaError(.fileReadCorruptFile)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}
