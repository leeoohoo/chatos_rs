import ChatOSCore
import CryptoKit
import Darwin
import Foundation

enum AgentMessageAttachmentFileExporter {
    static let maximumBytes = AgentMessageAttachmentDataLoader.maximumBytes
    private static let chunkSize = 256 * 1_024

    static func export(
        _ payload: ProjectAgentMessageAttachmentPayload,
        to destinationURL: URL
    ) async throws {
        let attachment = payload.attachment
        guard payload.localFileURL.isFileURL,
              destinationURL.isFileURL,
              attachment.size > 0,
              attachment.size <= maximumBytes,
              let sha256 = attachment.sha256,
              sha256.utf8.count == 64 else {
            throw AgentAttachmentPresentationError.invalidSize
        }
        do {
            try await AppCancellableDetachedWork.run {
                try exportSynchronously(
                    sourceURL: payload.localFileURL,
                    destinationURL: destinationURL,
                    expectedBytes: attachment.size,
                    expectedSHA256: sha256.lowercased()
                )
            }
        } catch is CancellationError {
            throw CancellationError()
        } catch ExportError.invalidSource {
            throw AgentAttachmentPresentationError.invalidSize
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
        guard sourceDescriptor >= 0 else { throw ExportError.invalidSource }
        defer { Darwin.close(sourceDescriptor) }

        var initialStatus = stat()
        guard fstat(sourceDescriptor, &initialStatus) == 0,
              (initialStatus.st_mode & S_IFMT) == S_IFREG,
              initialStatus.st_size == expectedBytes else {
            throw ExportError.invalidSource
        }

        let destinationDirectory = destinationURL.deletingLastPathComponent()
        try MediaDiskCapacityPolicy.ensureCapacity(
            in: destinationDirectory,
            writeBytes: Int64(expectedBytes)
        )
        let transaction = try AppOwnedExportTransaction(
            destinationURL: destinationURL,
            temporaryPrefix: ".chatos-agent-export-"
        )
        let destinationDescriptor = transaction.fileDescriptor

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
            guard count > 0 else { throw ExportError.invalidSource }
            buffer.withUnsafeBytes { bytes in
                hasher.update(bufferPointer: UnsafeRawBufferPointer(
                    rebasing: bytes.prefix(count)
                ))
            }
            try writeAll(
                descriptor: destinationDescriptor,
                bytes: buffer,
                count: count
            )
            remaining -= count
        }
        var trailingByte: UInt8 = 0
        while true {
            let trailingCount = Darwin.read(sourceDescriptor, &trailingByte, 1)
            if trailingCount == 0 { break }
            if trailingCount < 0, errno == EINTR { continue }
            throw ExportError.invalidSource
        }
        let digest = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        guard digest == expectedSHA256 else { throw ExportError.invalidSource }

        var finalStatus = stat()
        guard fstat(sourceDescriptor, &finalStatus) == 0,
              finalStatus.st_dev == initialStatus.st_dev,
              finalStatus.st_ino == initialStatus.st_ino,
              finalStatus.st_size == initialStatus.st_size,
              finalStatus.st_mtimespec.tv_sec == initialStatus.st_mtimespec.tv_sec,
              finalStatus.st_mtimespec.tv_nsec == initialStatus.st_mtimespec.tv_nsec else {
            throw ExportError.invalidSource
        }
        try Task.checkCancellation()
        guard fsync(destinationDescriptor) == 0 else { throw CocoaError(.fileWriteUnknown) }
        try transaction.install(as: destinationURL.lastPathComponent)
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

    private enum ExportError: Error {
        case invalidSource
    }
}
