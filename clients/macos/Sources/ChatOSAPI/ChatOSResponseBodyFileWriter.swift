import ChatOSCore
import Darwin
import Foundation

enum ChatOSResponseBodyFileWriter {
    private static let fileMode = mode_t(S_IRUSR | S_IWUSR)

    static func write(
        _ body: AsyncThrowingStream<Data, Error>,
        maximumBytes: Int,
        expectedBytes: Int? = nil,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil
    ) async throws -> GeneratedMediaFile {
        guard maximumBytes > 0,
              expectedBytes.map({ $0 > 0 && $0 <= maximumBytes }) ?? true else {
            throw MediaGenerationClientError.invalidVideoContent
        }
        let plannedBytes = expectedBytes ?? maximumBytes
        do {
            // The downloaded temporary file normally coexists with one final Story/History
            // copy. Keep room for that second copy on the common same-volume path.
            if let availableCapacityOverride {
                try MediaDiskCapacityPolicy.ensureCapacity(
                    availableBytes: availableCapacityOverride,
                    writeBytes: Int64(plannedBytes),
                    retainedBytes: Int64(plannedBytes)
                )
            } else {
                try MediaDiskCapacityPolicy.ensureCapacity(
                    in: temporaryDirectory,
                    writeBytes: Int64(plannedBytes),
                    retainedBytes: Int64(plannedBytes)
                )
            }
        } catch let error as MediaDiskCapacityError {
            throw MediaGenerationClientError.insufficientDiskSpace(error.localizedDescription)
        }
        let destination = try makeDestination(in: temporaryDirectory)
        var completed = false
        defer {
            try? destination.handle.close()
            if !completed { try? FileManager.default.removeItem(at: destination.url) }
        }

        var byteCount = 0
        do {
            for try await chunk in body {
                try Task.checkCancellation()
                guard !chunk.isEmpty,
                      chunk.count <= maximumBytes,
                      byteCount <= maximumBytes - chunk.count else {
                    throw MediaGenerationClientError.invalidVideoContent
                }
                try destination.handle.write(contentsOf: chunk)
                byteCount += chunk.count
            }
            try Task.checkCancellation()
            guard byteCount > 0,
                  expectedBytes.map({ byteCount == $0 }) ?? true else {
                throw MediaGenerationClientError.invalidVideoContent
            }
            try destination.handle.synchronize()
            try destination.handle.close()
            completed = true
            return GeneratedMediaFile(
                url: destination.url,
                byteCount: byteCount,
                deleteWhenReleased: true
            )
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as MediaGenerationClientError {
            throw error
        } catch {
            let error = error as NSError
            if (error.domain == NSPOSIXErrorDomain && error.code == Int(ENOSPC))
                || (error.domain == NSCocoaErrorDomain
                    && error.code == CocoaError.fileWriteOutOfSpace.rawValue) {
                throw MediaGenerationClientError.insufficientDiskSpace(
                    "写入过程中磁盘空间已耗尽。"
                )
            }
            throw MediaGenerationClientError.invalidVideoContent
        }
    }

    private static func makeDestination(in temporaryDirectory: URL) throws -> (url: URL, handle: FileHandle) {
        let url = temporaryDirectory.appendingPathComponent(
            "chatos-video-download-\(UUID().uuidString.lowercased()).tmp",
            isDirectory: false
        )
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC,
                fileMode
            )
        }
        guard descriptor >= 0 else {
            throw MediaGenerationClientError.invalidVideoContent
        }
        return (url, FileHandle(fileDescriptor: descriptor, closeOnDealloc: true))
    }
}
