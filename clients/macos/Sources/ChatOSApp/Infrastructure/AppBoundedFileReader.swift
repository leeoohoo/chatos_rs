import ChatOSCore
import Darwin
import Foundation

enum AppBoundedFileReader {
    private static let chunkSize = 256 * 1_024

    static func read(_ url: URL, maximumBytes: Int) throws -> Data {
        guard maximumBytes > 0 else { throw AppBoundedFileReadError.invalidLimit }
        try Task.checkCancellation()
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard values.isRegularFile == true,
              let fileSize = values.fileSize,
              fileSize >= 0 else {
            throw AppBoundedFileReadError.notRegularFile
        }
        guard fileSize <= maximumBytes else {
            throw AppBoundedFileReadError.fileTooLarge(maximumBytes: maximumBytes)
        }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }

        let readLimit = maximumBytes == Int.max ? Int.max : maximumBytes + 1
        var data = Data()
        data.reserveCapacity(min(fileSize, maximumBytes))
        while data.count < readLimit {
            try Task.checkCancellation()
            let requestedBytes = min(Self.chunkSize, readLimit - data.count)
            guard let chunk = try handle.read(upToCount: requestedBytes),
                  !chunk.isEmpty else {
                break
            }
            data.append(chunk)
            guard data.count <= maximumBytes else {
                throw AppBoundedFileReadError.fileTooLarge(maximumBytes: maximumBytes)
            }
        }
        try Task.checkCancellation()
        return data
    }

    static func copy(
        _ sourceURL: URL,
        to destinationURL: URL,
        expectedBytes: Int,
        maximumBytes: Int,
        availableCapacityOverride: Int64? = nil
    ) throws {
        guard maximumBytes > 0, expectedBytes > 0, expectedBytes <= maximumBytes else {
            throw AppBoundedFileReadError.fileTooLarge(maximumBytes: maximumBytes)
        }
        try Task.checkCancellation()
        let sourceDescriptor = sourceURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard sourceDescriptor >= 0 else {
            throw AppBoundedFileReadError.notRegularFile
        }
        let source = FileHandle(fileDescriptor: sourceDescriptor, closeOnDealloc: true)
        defer { try? source.close() }
        var status = stat()
        guard fstat(sourceDescriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_size == expectedBytes else {
            throw AppBoundedFileReadError.notRegularFile
        }

        let destinationDirectory = destinationURL.deletingLastPathComponent()
        if let availableCapacityOverride {
            try MediaDiskCapacityPolicy.ensureCapacity(
                availableBytes: availableCapacityOverride,
                writeBytes: Int64(expectedBytes)
            )
        } else {
            try MediaDiskCapacityPolicy.ensureCapacity(
                in: destinationDirectory,
                writeBytes: Int64(expectedBytes)
            )
        }

        let destinationDescriptor = destinationURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC,
                mode_t(S_IRUSR | S_IWUSR)
            )
        }
        guard destinationDescriptor >= 0 else { throw CocoaError(.fileWriteUnknown) }
        let destination = FileHandle(fileDescriptor: destinationDescriptor, closeOnDealloc: true)
        var completed = false
        defer {
            try? destination.close()
            if !completed { try? FileManager.default.removeItem(at: destinationURL) }
        }

        var remaining = expectedBytes
        while remaining > 0 {
            try Task.checkCancellation()
            let chunk = try source.read(upToCount: min(chunkSize, remaining)) ?? Data()
            guard !chunk.isEmpty else { throw AppBoundedFileReadError.notRegularFile }
            try destination.write(contentsOf: chunk)
            remaining -= chunk.count
        }
        try destination.synchronize()
        try destination.close()
        completed = true
    }
}

enum AppBoundedFileReadError: LocalizedError {
    case invalidLimit
    case notRegularFile
    case fileTooLarge(maximumBytes: Int)

    var errorDescription: String? {
        switch self {
        case .invalidLimit:
            "文件读取上限无效。"
        case .notRegularFile:
            "目标不是可读取的普通文件。"
        case let .fileTooLarge(maximumBytes):
            "文件超过允许的读取上限（\(maximumBytes) 字节）。"
        }
    }
}

enum AppCancellableDetachedWork {
    static func run<Value: Sendable>(
        priority: TaskPriority = .userInitiated,
        operation: @escaping @Sendable () throws -> Value
    ) async throws -> Value {
        let task = Task.detached(priority: priority) {
            try Task.checkCancellation()
            return try operation()
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }

    static func runAsync<Value: Sendable>(
        priority: TaskPriority = .userInitiated,
        operation: @escaping @Sendable () async throws -> Value
    ) async throws -> Value {
        let task = Task.detached(priority: priority) {
            try Task.checkCancellation()
            return try await operation()
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }
}
