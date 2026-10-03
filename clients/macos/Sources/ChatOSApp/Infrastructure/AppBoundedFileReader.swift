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
