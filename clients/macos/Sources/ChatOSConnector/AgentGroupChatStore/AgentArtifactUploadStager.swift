import ChatOSCore
import CryptoKit
import Darwin
import Foundation

struct StagedAgentArtifact: Sendable, Equatable {
    let token: String
    let fileURL: URL
}

enum AgentArtifactUploadStager {
    private static let chunkSize = 256 * 1_024

    static func stage(
        from sourceURL: URL,
        in stagingDirectoryURL: URL,
        expectedBytes: Int,
        expectedSHA256: String
    ) async throws -> StagedAgentArtifact {
        guard sourceURL.isFileURL,
              stagingDirectoryURL.isFileURL,
              expectedBytes > 0,
              expectedBytes <= AgentCommunicationPolicy.standard.maximumDocumentBytes,
              isSHA256(expectedSHA256) else {
            throw AgentArtifactUploadStagingError.invalidSource
        }
        try Task.checkCancellation()
        let task = Task.detached(priority: .utility) {
            try stageSynchronously(
                from: sourceURL,
                in: stagingDirectoryURL,
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

    static func discard(_ staged: StagedAgentArtifact) {
        guard staged.token.utf8.allSatisfy(isTokenByte),
              staged.fileURL.lastPathComponent == "\(staged.token).staged" else { return }
        _ = staged.fileURL.withUnsafeFileSystemRepresentation { path in
            path.map(Darwin.unlink) ?? -1
        }
    }

    private nonisolated static func stageSynchronously(
        from sourceURL: URL,
        in stagingDirectoryURL: URL,
        expectedBytes: Int,
        expectedSHA256: String
    ) throws -> StagedAgentArtifact {
        try Task.checkCancellation()
        try preparePrivateDirectory(stagingDirectoryURL)
        let token = UUID().uuidString.lowercased()
        let temporaryURL = stagingDirectoryURL.appendingPathComponent(".\(token).tmp")
        let finalURL = stagingDirectoryURL.appendingPathComponent("\(token).staged")

        let source = sourceURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard source >= 0 else { throw AgentArtifactUploadStagingError.invalidSource }
        defer { Darwin.close(source) }

        var initialStatus = stat()
        guard fstat(source, &initialStatus) == 0,
              (initialStatus.st_mode & S_IFMT) == S_IFREG,
              initialStatus.st_nlink == 1,
              initialStatus.st_size == expectedBytes else {
            throw AgentArtifactUploadStagingError.invalidSource
        }

        let destination = temporaryURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
                mode_t(0o600)
            )
        }
        guard destination >= 0 else { throw AgentArtifactUploadStagingError.stagingUnavailable }
        var published = false
        defer {
            Darwin.close(destination)
            if !published {
                _ = temporaryURL.withUnsafeFileSystemRepresentation { path in
                    path.map(Darwin.unlink) ?? -1
                }
                _ = finalURL.withUnsafeFileSystemRepresentation { path in
                    path.map(Darwin.unlink) ?? -1
                }
            }
        }

        var hasher = SHA256()
        var buffer = [UInt8](repeating: 0, count: chunkSize)
        var remaining = expectedBytes
        while remaining > 0 {
            try Task.checkCancellation()
            let requested = min(buffer.count, remaining)
            let count = buffer.withUnsafeMutableBytes { bytes in
                Darwin.read(source, bytes.baseAddress, requested)
            }
            if count < 0, errno == EINTR { continue }
            guard count > 0 else { throw AgentArtifactUploadStagingError.invalidSource }
            buffer.withUnsafeBytes { bytes in
                hasher.update(bufferPointer: UnsafeRawBufferPointer(rebasing: bytes.prefix(count)))
            }
            try writeAll(buffer, count: count, descriptor: destination)
            remaining -= count
        }

        var trailingByte: UInt8 = 0
        while true {
            let count = Darwin.read(source, &trailingByte, 1)
            if count == 0 { break }
            if count < 0, errno == EINTR { continue }
            throw AgentArtifactUploadStagingError.invalidSource
        }
        let digest = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        guard digest == expectedSHA256 else {
            throw AgentArtifactUploadStagingError.invalidSource
        }

        var finalStatus = stat()
        guard fstat(source, &finalStatus) == 0,
              sameIdentity(initialStatus, finalStatus),
              Darwin.fsync(destination) == 0 else {
            throw AgentArtifactUploadStagingError.invalidSource
        }
        try Task.checkCancellation()
        let renamed: Int32 = temporaryURL.withUnsafeFileSystemRepresentation { temporaryPath in
            finalURL.withUnsafeFileSystemRepresentation { finalPath -> Int32 in
                guard let temporaryPath, let finalPath else { return Int32(-1) }
                return Darwin.rename(temporaryPath, finalPath)
            }
        }
        guard renamed == 0 else { throw AgentArtifactUploadStagingError.stagingUnavailable }
        try syncDirectory(stagingDirectoryURL)
        published = true
        return .init(token: token, fileURL: finalURL)
    }

    private static func preparePrivateDirectory(_ directoryURL: URL) throws {
        let result = directoryURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            if Darwin.mkdir(path, mode_t(0o700)) == 0 || errno == EEXIST {
                return 0
            }
            return -1
        }
        guard result == 0 else { throw AgentArtifactUploadStagingError.stagingUnavailable }
        var status = stat()
        let inspected = directoryURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.lstat(path, &status)
        }
        guard inspected == 0,
              (status.st_mode & S_IFMT) == S_IFDIR,
              status.st_uid == geteuid() else {
            throw AgentArtifactUploadStagingError.stagingUnavailable
        }
        let restricted = directoryURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return Int32(-1) }
            return Darwin.chmod(path, mode_t(0o700))
        }
        guard restricted == 0 else {
            throw AgentArtifactUploadStagingError.stagingUnavailable
        }
        var finalStatus = stat()
        let rechecked = directoryURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return Int32(-1) }
            return Darwin.lstat(path, &finalStatus)
        }
        guard rechecked == 0,
              (finalStatus.st_mode & S_IFMT) == S_IFDIR,
              finalStatus.st_uid == geteuid(),
              finalStatus.st_mode & 0o077 == 0 else {
            throw AgentArtifactUploadStagingError.stagingUnavailable
        }
    }

    private static func syncDirectory(_ directoryURL: URL) throws {
        let descriptor = directoryURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw AgentArtifactUploadStagingError.stagingUnavailable }
        defer { Darwin.close(descriptor) }
        guard Darwin.fsync(descriptor) == 0 else {
            throw AgentArtifactUploadStagingError.stagingUnavailable
        }
    }

    private static func writeAll(
        _ buffer: [UInt8],
        count: Int,
        descriptor: Int32
    ) throws {
        var offset = 0
        while offset < count {
            try Task.checkCancellation()
            let written = buffer.withUnsafeBytes { bytes in
                Darwin.write(descriptor, bytes.baseAddress?.advanced(by: offset), count - offset)
            }
            if written < 0, errno == EINTR { continue }
            guard written > 0 else { throw AgentArtifactUploadStagingError.stagingUnavailable }
            offset += written
        }
    }

    private static func sameIdentity(_ lhs: stat, _ rhs: stat) -> Bool {
        lhs.st_dev == rhs.st_dev
            && lhs.st_ino == rhs.st_ino
            && lhs.st_size == rhs.st_size
            && lhs.st_mtimespec.tv_sec == rhs.st_mtimespec.tv_sec
            && lhs.st_mtimespec.tv_nsec == rhs.st_mtimespec.tv_nsec
            && lhs.st_ctimespec.tv_sec == rhs.st_ctimespec.tv_sec
            && lhs.st_ctimespec.tv_nsec == rhs.st_ctimespec.tv_nsec
    }

    private static func isSHA256(_ value: String) -> Bool {
        value.utf8.count == 64 && value.utf8.allSatisfy {
            (48...57).contains($0) || (65...70).contains($0) || (97...102).contains($0)
        }
    }

    private static func isTokenByte(_ byte: UInt8) -> Bool {
        (48...57).contains(byte) || (97...102).contains(byte) || byte == 45
    }
}

enum AgentArtifactUploadStagingError: Error {
    case invalidSource
    case stagingUnavailable
}
