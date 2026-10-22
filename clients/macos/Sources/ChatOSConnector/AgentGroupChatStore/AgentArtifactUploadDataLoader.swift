import ChatOSCore
import CryptoKit
import Darwin
import Foundation

enum AgentArtifactUploadDataLoader {
    private static let chunkSize = 256 * 1_024

    static func load(
        from fileURL: URL,
        expectedBytes: Int,
        expectedSHA256: String
    ) async throws -> Data {
        guard fileURL.isFileURL,
              expectedBytes > 0,
              expectedBytes <= AgentCommunicationPolicy.standard.maximumDocumentBytes,
              isSHA256(expectedSHA256) else {
            throw AgentArtifactUploadDataError.invalidSource
        }
        try Task.checkCancellation()
        let task = Task.detached(priority: .utility) {
            try loadSynchronously(
                from: fileURL,
                expectedBytes: expectedBytes,
                expectedSHA256: expectedSHA256.lowercased()
            )
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }

    private nonisolated static func loadSynchronously(
        from fileURL: URL,
        expectedBytes: Int,
        expectedSHA256: String
    ) throws -> Data {
        try Task.checkCancellation()
        let descriptor = fileURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw AgentArtifactUploadDataError.invalidSource }
        defer { Darwin.close(descriptor) }
        var initialStatus = stat()
        guard fstat(descriptor, &initialStatus) == 0,
              (initialStatus.st_mode & S_IFMT) == S_IFREG,
              initialStatus.st_size == expectedBytes else {
            throw AgentArtifactUploadDataError.invalidSource
        }

        var data = Data()
        data.reserveCapacity(expectedBytes)
        var hasher = SHA256()
        var buffer = [UInt8](repeating: 0, count: chunkSize)
        var remaining = expectedBytes
        while remaining > 0 {
            try Task.checkCancellation()
            let requested = min(buffer.count, remaining)
            let count = buffer.withUnsafeMutableBytes { bytes in
                Darwin.read(descriptor, bytes.baseAddress, requested)
            }
            if count < 0, errno == EINTR { continue }
            guard count > 0 else { throw AgentArtifactUploadDataError.invalidSource }
            buffer.withUnsafeBytes { bytes in
                hasher.update(bufferPointer: UnsafeRawBufferPointer(
                    rebasing: bytes.prefix(count)
                ))
            }
            data.append(buffer, count: count)
            remaining -= count
        }

        var trailingByte: UInt8 = 0
        while true {
            let count = Darwin.read(descriptor, &trailingByte, 1)
            if count == 0 { break }
            if count < 0, errno == EINTR { continue }
            throw AgentArtifactUploadDataError.invalidSource
        }
        let digest = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        guard digest == expectedSHA256 else {
            throw AgentArtifactUploadDataError.invalidSource
        }

        var finalStatus = stat()
        guard fstat(descriptor, &finalStatus) == 0,
              finalStatus.st_dev == initialStatus.st_dev,
              finalStatus.st_ino == initialStatus.st_ino,
              finalStatus.st_size == initialStatus.st_size,
              finalStatus.st_mtimespec.tv_sec == initialStatus.st_mtimespec.tv_sec,
              finalStatus.st_mtimespec.tv_nsec == initialStatus.st_mtimespec.tv_nsec,
              finalStatus.st_ctimespec.tv_sec == initialStatus.st_ctimespec.tv_sec,
              finalStatus.st_ctimespec.tv_nsec == initialStatus.st_ctimespec.tv_nsec else {
            throw AgentArtifactUploadDataError.invalidSource
        }
        try Task.checkCancellation()
        return data
    }

    private static func isSHA256(_ value: String) -> Bool {
        value.utf8.count == 64 && value.utf8.allSatisfy {
            (48...57).contains($0) || (65...70).contains($0) || (97...102).contains($0)
        }
    }
}

enum AgentArtifactUploadDataError: Error {
    case invalidSource
}
