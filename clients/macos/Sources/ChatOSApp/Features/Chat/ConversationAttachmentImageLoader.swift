import ChatOSMediaWorkerProtocol
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
        ownerUserID: String,
        maximumDisplayPixelSize: Int = 2_048,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared
    ) async throws -> AppDecodedThumbnail {
        let verified = try await AppCancellableDetachedWork.run {
            try verifySource(
                url: url,
                sha256: sha256,
                expectedBytes: expectedBytes
            )
        }
        let decoded = try await AppImageThumbnailLoader.loadLocalFileUsingWorker(
            url,
            sourceIdentity: verified.workerIdentity,
            ownerUserID: ownerUserID,
            maximumBytes: maximumBytes,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumDisplayPixelSize: maximumDisplayPixelSize,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride,
            mediaWorker: mediaWorker
        )
        try await AppCancellableDetachedWork.run {
            try verifyUnchanged(url: url, expected: verified)
        }
        return decoded
    }

    private struct VerifiedSource: Sendable {
        let workerIdentity: MediaWorkerFileIdentity
        let modifiedSeconds: Int64
        let modifiedNanoseconds: Int64
    }

    private nonisolated static func verifySource(
        url: URL,
        sha256: String?,
        expectedBytes: Int
    ) throws -> VerifiedSource {
        guard url.isFileURL,
              expectedBytes > 0,
              expectedBytes <= maximumBytes,
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
              initialStatus.st_uid == geteuid(),
              initialStatus.st_size == expectedBytes else {
            throw CocoaError(.fileReadCorruptFile)
        }
        guard try digest(descriptor: descriptor, expectedBytes: expectedBytes) == sha256.lowercased() else {
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
        return .init(
            workerIdentity: .init(
                device: UInt64(initialStatus.st_dev),
                inode: UInt64(initialStatus.st_ino),
                byteCount: Int64(initialStatus.st_size)
            ),
            modifiedSeconds: Int64(initialStatus.st_mtimespec.tv_sec),
            modifiedNanoseconds: Int64(initialStatus.st_mtimespec.tv_nsec)
        )
    }

    private nonisolated static func verifyUnchanged(
        url: URL,
        expected: VerifiedSource
    ) throws {
        try Task.checkCancellation()
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CocoaError(.fileReadNoSuchFile) }
        defer { Darwin.close(descriptor) }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_uid == geteuid(),
              UInt64(status.st_dev) == expected.workerIdentity.device,
              UInt64(status.st_ino) == expected.workerIdentity.inode,
              Int64(status.st_size) == expected.workerIdentity.byteCount,
              Int64(status.st_mtimespec.tv_sec) == expected.modifiedSeconds,
              Int64(status.st_mtimespec.tv_nsec) == expected.modifiedNanoseconds else {
            throw CocoaError(.fileReadCorruptFile)
        }
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
