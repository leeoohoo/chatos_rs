import Foundation

/// A conservative admission check for large media writes. This is deliberately a
/// preflight rather than a reservation: the write path must still handle ENOSPC if
/// another process consumes capacity after the check.
public enum MediaDiskCapacityPolicy {
    public static let minimumFreeBytesAfterWrite: Int64 = 256 * 1_024 * 1_024

    public static func availableCapacity(at directory: URL) throws -> Int64 {
        guard directory.isFileURL else { throw MediaDiskCapacityError.capacityUnavailable }
        guard let attributes = try? FileManager.default.attributesOfFileSystem(
            forPath: directory.path
        ) else {
            throw MediaDiskCapacityError.capacityUnavailable
        }
        guard let capacity = attributes[.systemFreeSize] as? NSNumber,
              capacity.int64Value >= 0 else {
            throw MediaDiskCapacityError.capacityUnavailable
        }
        return capacity.int64Value
    }

    public static func ensureCapacity(
        availableBytes: Int64,
        writeBytes: Int64,
        retainedBytes: Int64 = 0,
        minimumFreeBytes: Int64 = minimumFreeBytesAfterWrite
    ) throws {
        guard availableBytes >= 0, writeBytes > 0, retainedBytes >= 0,
              minimumFreeBytes >= 0 else {
            throw MediaDiskCapacityError.invalidRequirement
        }
        let (writeAndRetained, firstOverflow) = writeBytes.addingReportingOverflow(retainedBytes)
        let (requiredBytes, secondOverflow) = writeAndRetained.addingReportingOverflow(minimumFreeBytes)
        guard !firstOverflow, !secondOverflow else {
            throw MediaDiskCapacityError.invalidRequirement
        }
        guard availableBytes >= requiredBytes else {
            throw MediaDiskCapacityError.insufficientCapacity(
                requiredBytes: requiredBytes,
                availableBytes: availableBytes
            )
        }
    }

    public static func ensureCapacity(
        in directory: URL,
        writeBytes: Int64,
        retainedBytes: Int64 = 0,
        minimumFreeBytes: Int64 = minimumFreeBytesAfterWrite
    ) throws {
        try ensureCapacity(
            availableBytes: availableCapacity(at: directory),
            writeBytes: writeBytes,
            retainedBytes: retainedBytes,
            minimumFreeBytes: minimumFreeBytes
        )
    }
}

public enum MediaDiskCapacityError: LocalizedError, Sendable, Equatable {
    case invalidRequirement
    case capacityUnavailable
    case insufficientCapacity(requiredBytes: Int64, availableBytes: Int64)

    public var errorDescription: String? {
        switch self {
        case .invalidRequirement:
            "媒体文件的磁盘空间预算无效。"
        case .capacityUnavailable:
            "无法确认目标磁盘的可用空间。"
        case let .insufficientCapacity(requiredBytes, availableBytes):
            "磁盘空间不足，需要至少 \(requiredBytes) 字节，当前可用 \(availableBytes) 字节。"
        }
    }
}
