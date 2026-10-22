import ChatOSCore
import CryptoKit
import Darwin
import Foundation

enum AgentArtifactFileExporter {
    static let maximumBytes = 2 * 1_024 * 1_024
    private static let chunkSize = 256 * 1_024

    static func export(
        _ reference: AgentArtifactFileReference,
        to destinationURL: URL
    ) async throws {
        guard reference.fileURL.isFileURL,
              destinationURL.isFileURL,
              reference.size > 0,
              reference.size <= maximumBytes,
              reference.sha256.utf8.count == 64 else {
            throw AgentArtifactFileExportError.invalidSource
        }
        try await AppCancellableDetachedWork.run {
            try exportSynchronously(
                sourceURL: reference.fileURL,
                destinationURL: destinationURL,
                expectedBytes: reference.size,
                expectedSHA256: reference.sha256.lowercased()
            )
        }
    }

    private nonisolated static func exportSynchronously(
        sourceURL: URL,
        destinationURL: URL,
        expectedBytes: Int,
        expectedSHA256: String
    ) throws {
        try Task.checkCancellation()
        let sourceDescriptor = sourceURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard sourceDescriptor >= 0 else { throw AgentArtifactFileExportError.invalidSource }
        defer { Darwin.close(sourceDescriptor) }

        var initialStatus = stat()
        guard fstat(sourceDescriptor, &initialStatus) == 0,
              (initialStatus.st_mode & S_IFMT) == S_IFREG,
              initialStatus.st_size == expectedBytes else {
            throw AgentArtifactFileExportError.invalidSource
        }

        let destinationDirectory = destinationURL.deletingLastPathComponent()
        try MediaDiskCapacityPolicy.ensureCapacity(
            in: destinationDirectory,
            writeBytes: Int64(expectedBytes)
        )
        let temporaryURL = destinationDirectory.appendingPathComponent(
            ".chatos-artifact-export-\(UUID().uuidString).tmp"
        )
        let destinationDescriptor = temporaryURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
                mode_t(S_IRUSR | S_IWUSR)
            )
        }
        guard destinationDescriptor >= 0 else { throw CocoaError(.fileWriteUnknown) }
        var installed = false
        defer {
            Darwin.close(destinationDescriptor)
            if !installed { try? FileManager.default.removeItem(at: temporaryURL) }
        }

        var hasher = SHA256()
        var buffer = [UInt8](repeating: 0, count: chunkSize)
        var remaining = expectedBytes
        while remaining > 0 {
            try Task.checkCancellation()
            let requested = min(buffer.count, remaining)
            let count = buffer.withUnsafeMutableBytes { bytes in
                Darwin.read(sourceDescriptor, bytes.baseAddress, requested)
            }
            if count < 0, errno == EINTR { continue }
            guard count > 0 else { throw AgentArtifactFileExportError.invalidSource }
            buffer.withUnsafeBytes { bytes in
                hasher.update(bufferPointer: UnsafeRawBufferPointer(
                    rebasing: bytes.prefix(count)
                ))
            }
            try writeAll(descriptor: destinationDescriptor, bytes: buffer, count: count)
            remaining -= count
        }

        var trailingByte: UInt8 = 0
        while true {
            let trailingCount = Darwin.read(sourceDescriptor, &trailingByte, 1)
            if trailingCount == 0 { break }
            if trailingCount < 0, errno == EINTR { continue }
            throw AgentArtifactFileExportError.invalidSource
        }
        let digest = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        guard digest == expectedSHA256 else {
            throw AgentArtifactFileExportError.invalidSource
        }

        var finalStatus = stat()
        guard fstat(sourceDescriptor, &finalStatus) == 0,
              finalStatus.st_dev == initialStatus.st_dev,
              finalStatus.st_ino == initialStatus.st_ino,
              finalStatus.st_size == initialStatus.st_size,
              finalStatus.st_mtimespec.tv_sec == initialStatus.st_mtimespec.tv_sec,
              finalStatus.st_mtimespec.tv_nsec == initialStatus.st_mtimespec.tv_nsec else {
            throw AgentArtifactFileExportError.invalidSource
        }
        try Task.checkCancellation()
        guard fsync(destinationDescriptor) == 0 else { throw CocoaError(.fileWriteUnknown) }
        let renameResult: Int32 = temporaryURL.withUnsafeFileSystemRepresentation { temporaryPath in
            destinationURL.withUnsafeFileSystemRepresentation { destinationPath in
                guard let temporaryPath, let destinationPath else { return Int32(-1) }
                return Darwin.rename(temporaryPath, destinationPath)
            }
        }
        guard renameResult == 0 else { throw CocoaError(.fileWriteUnknown) }
        installed = true
    }

    private nonisolated static func writeAll(
        descriptor: Int32,
        bytes: [UInt8],
        count: Int
    ) throws {
        var written = 0
        while written < count {
            try Task.checkCancellation()
            let result = bytes.withUnsafeBytes { buffer in
                Darwin.write(
                    descriptor,
                    buffer.baseAddress?.advanced(by: written),
                    count - written
                )
            }
            if result < 0, errno == EINTR { continue }
            guard result > 0 else { throw CocoaError(.fileWriteUnknown) }
            written += result
        }
    }
}

enum AgentArtifactFileExportError: LocalizedError {
    case invalidSource

    var errorDescription: String? {
        "本地文档文件与记录的大小或 SHA-256 不一致。"
    }
}
