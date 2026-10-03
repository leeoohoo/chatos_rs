import Foundation

enum NativeBoundedFileReader {
    private static let chunkSize = 256 * 1_024

    static func read(_ url: URL, maximumBytes: Int) throws -> Data {
        guard maximumBytes > 0 else { throw NativeBoundedFileReadError.invalidLimit }
        try Task.checkCancellation()
        let values = try url.resourceValues(forKeys: [
            .isRegularFileKey,
            .isSymbolicLinkKey,
            .fileSizeKey,
        ])
        guard values.isRegularFile == true,
              values.isSymbolicLink != true,
              let fileSize = values.fileSize,
              fileSize >= 0 else {
            throw NativeBoundedFileReadError.notRegularFile
        }
        guard fileSize <= maximumBytes else {
            throw NativeBoundedFileReadError.fileTooLarge(maximumBytes: maximumBytes)
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
                throw NativeBoundedFileReadError.fileTooLarge(maximumBytes: maximumBytes)
            }
        }
        try Task.checkCancellation()
        return data
    }
}

enum NativeBoundedFileReadError: LocalizedError {
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
