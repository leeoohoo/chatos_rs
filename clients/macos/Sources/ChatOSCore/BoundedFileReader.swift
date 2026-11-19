import Darwin
import Foundation

public enum BoundedFileReader {
    private static let chunkSize = 256 * 1_024

    public static func read(_ url: URL, maximumBytes: Int) throws -> Data {
        guard url.isFileURL else { throw BoundedFileReadError.notRegularFile }
        guard maximumBytes > 0 else { throw BoundedFileReadError.invalidLimit }
        try Task.checkCancellation()
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw BoundedFileReadError.notRegularFile }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? handle.close() }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_size >= 0 else {
            throw BoundedFileReadError.notRegularFile
        }
        let fileSize = Int(status.st_size)
        guard fileSize <= maximumBytes else {
            throw BoundedFileReadError.fileTooLarge(maximumBytes: maximumBytes)
        }

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
                throw BoundedFileReadError.fileTooLarge(maximumBytes: maximumBytes)
            }
        }
        try Task.checkCancellation()
        guard data.count == fileSize else { throw BoundedFileReadError.notRegularFile }
        return data
    }
}

public enum BoundedFileReadError: LocalizedError, Equatable {
    case invalidLimit
    case notRegularFile
    case fileTooLarge(maximumBytes: Int)

    public var errorDescription: String? {
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
