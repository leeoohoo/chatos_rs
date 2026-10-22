import ChatOSCore
import Darwin
import Foundation

enum ChatOSResponseBodyFileWriter {
    private static let fileMode = mode_t(S_IRUSR | S_IWUSR)
    private static let directoryMode = mode_t(S_IRWXU)
    private static let artifactPrefix = ".chatos-video-download-"
    private static let leaseFilename = ".lease"
    private static let payloadFilename = "payload.tmp"
    private static let orphanRetention: TimeInterval = 24 * 60 * 60
    private static let maximumCleanupCandidates = 64

    private final class ArtifactOwner: @unchecked Sendable {
        let directoryURL: URL
        let payloadURL: URL

        private let lock = NSLock()
        private var leaseDescriptor: Int32?
        private var removed = false

        init(directoryURL: URL, payloadURL: URL, leaseDescriptor: Int32) {
            self.directoryURL = directoryURL
            self.payloadURL = payloadURL
            self.leaseDescriptor = leaseDescriptor
        }

        deinit {
            removeArtifact()
        }

        func removeArtifact() {
            lock.lock()
            guard !removed else {
                lock.unlock()
                return
            }
            removed = true
            let descriptor = leaseDescriptor
            leaseDescriptor = nil
            lock.unlock()

            // Keep the lease through removal. A concurrent maintenance pass therefore cannot
            // mistake a live result for a crash orphan while its directory is being deleted.
            try? FileManager.default.removeItem(at: directoryURL)
            if let descriptor {
                _ = flock(descriptor, LOCK_UN)
                _ = Darwin.close(descriptor)
            }
        }
    }

    private struct Destination {
        let owner: ArtifactOwner
        let handle: FileHandle
    }

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
        _ = reconcileOrphanedArtifacts(
            in: temporaryDirectory,
            olderThan: Date().addingTimeInterval(-orphanRetention),
            maximumCandidates: maximumCleanupCandidates
        )
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
            if !completed { destination.owner.removeArtifact() }
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
            let owner = destination.owner
            return GeneratedMediaFile(
                url: owner.payloadURL,
                byteCount: byteCount,
                deleteWhenReleased: true,
                cleanup: { owner.removeArtifact() }
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

    @discardableResult
    static func reconcileOrphanedArtifacts(
        in temporaryDirectory: URL,
        olderThan cutoff: Date,
        maximumCandidates: Int = maximumCleanupCandidates
    ) -> Int {
        guard maximumCandidates > 0,
              let children = try? FileManager.default.contentsOfDirectory(
                at: temporaryDirectory,
                includingPropertiesForKeys: nil,
                options: []
              ) else { return 0 }

        let candidates = children.compactMap { url -> (URL, stat)? in
            guard isCanonicalArtifactName(url.lastPathComponent),
                  let status = secureStatus(url),
                  (status.st_mode & S_IFMT) == S_IFDIR,
                  status.st_uid == geteuid(),
                  modificationDate(status) < cutoff else { return nil }
            return (url, status)
        }
        .sorted { $0.0.lastPathComponent < $1.0.lastPathComponent }
        .prefix(maximumCandidates)

        var removed = 0
        for (candidate, directoryStatus) in candidates {
            if reconcile(candidate, originalDirectoryStatus: directoryStatus) {
                removed += 1
            }
        }
        return removed
    }

    private static func reconcile(_ directory: URL, originalDirectoryStatus: stat) -> Bool {
        guard let children = try? FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: nil,
            options: []
        ) else { return false }
        let names = Set(children.map(\.lastPathComponent))
        guard names.isSubset(of: [leaseFilename, payloadFilename]) else { return false }

        let leaseURL = directory.appendingPathComponent(leaseFilename, isDirectory: false)
        guard names.contains(leaseFilename) else {
            // A crash between mkdir and lease creation leaves an empty directory. Any nonempty
            // lease-less directory predates this protocol or is malformed and is preserved.
            guard names.isEmpty else { return false }
            return removeDirectoryIfUnchanged(directory, originalStatus: originalDirectoryStatus)
        }
        guard let leasePathStatus = secureStatus(leaseURL),
              isOwnedSingleLinkRegularFile(leasePathStatus) else { return false }

        if names.contains(payloadFilename) {
            let payloadURL = directory.appendingPathComponent(payloadFilename, isDirectory: false)
            guard let payloadStatus = secureStatus(payloadURL),
                  isOwnedSingleLinkRegularFile(payloadStatus) else { return false }
        }

        let descriptor = leaseURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { return false }
        defer {
            _ = flock(descriptor, LOCK_UN)
            _ = Darwin.close(descriptor)
        }
        var descriptorStatus = stat()
        guard fstat(descriptor, &descriptorStatus) == 0,
              isOwnedSingleLinkRegularFile(descriptorStatus),
              descriptorStatus.st_dev == leasePathStatus.st_dev,
              descriptorStatus.st_ino == leasePathStatus.st_ino,
              flock(descriptor, LOCK_EX | LOCK_NB) == 0,
              directoryIsUnchanged(directory, originalStatus: originalDirectoryStatus)
        else { return false }

        // Recheck the small, closed schema after acquiring the lease so a path replacement
        // cannot widen deletion beyond files created by this writer.
        guard let lockedChildren = try? FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: nil,
            options: []
        ), Set(lockedChildren.map(\.lastPathComponent)) == names,
           let lockedLeaseStatus = secureStatus(leaseURL),
           isOwnedSingleLinkRegularFile(lockedLeaseStatus),
           lockedLeaseStatus.st_dev == descriptorStatus.st_dev,
           lockedLeaseStatus.st_ino == descriptorStatus.st_ino else { return false }
        if names.contains(payloadFilename) {
            let payloadURL = directory.appendingPathComponent(payloadFilename, isDirectory: false)
            guard let payloadStatus = secureStatus(payloadURL),
                  isOwnedSingleLinkRegularFile(payloadStatus) else { return false }
        }
        return (try? FileManager.default.removeItem(at: directory)) != nil
    }

    private static func removeDirectoryIfUnchanged(_ directory: URL, originalStatus: stat) -> Bool {
        guard directoryIsUnchanged(directory, originalStatus: originalStatus),
              let children = try? FileManager.default.contentsOfDirectory(
                at: directory,
                includingPropertiesForKeys: nil,
                options: []
              ), children.isEmpty else { return false }
        return (try? FileManager.default.removeItem(at: directory)) != nil
    }

    private static func directoryIsUnchanged(_ directory: URL, originalStatus: stat) -> Bool {
        guard let current = secureStatus(directory),
              (current.st_mode & S_IFMT) == S_IFDIR,
              current.st_uid == geteuid() else { return false }
        return current.st_dev == originalStatus.st_dev && current.st_ino == originalStatus.st_ino
    }

    private static func isCanonicalArtifactName(_ name: String) -> Bool {
        guard name.hasPrefix(artifactPrefix) else { return false }
        let value = String(name.dropFirst(artifactPrefix.count))
        guard value.count == 36, value == value.lowercased(), let id = UUID(uuidString: value)
        else { return false }
        return id.uuidString.lowercased() == value
    }

    private static func secureStatus(_ url: URL) -> stat? {
        var status = stat()
        let result = url.withUnsafeFileSystemRepresentation { path in
            path.map { Darwin.lstat($0, &status) } ?? -1
        }
        return result == 0 ? status : nil
    }

    private static func isOwnedSingleLinkRegularFile(_ status: stat) -> Bool {
        (status.st_mode & S_IFMT) == S_IFREG
            && status.st_nlink == 1
            && status.st_uid == geteuid()
    }

    private static func modificationDate(_ status: stat) -> Date {
        Date(
            timeIntervalSince1970: TimeInterval(status.st_mtimespec.tv_sec)
                + TimeInterval(status.st_mtimespec.tv_nsec) / 1_000_000_000
        )
    }

    private static func makeDestination(in temporaryDirectory: URL) throws -> Destination {
        let directoryURL = temporaryDirectory.appendingPathComponent(
            "\(artifactPrefix)\(UUID().uuidString.lowercased())",
            isDirectory: true
        )
        let created = directoryURL.withUnsafeFileSystemRepresentation { path -> Bool in
            guard let path else { return false }
            return Darwin.mkdir(path, directoryMode) == 0
        }
        guard created else { throw MediaGenerationClientError.invalidVideoContent }

        var leaseDescriptor: Int32?
        do {
            try secureDirectory(directoryURL)
            let leaseURL = directoryURL.appendingPathComponent(leaseFilename, isDirectory: false)
            let descriptor = try createLockedFile(at: leaseURL, flags: O_RDWR)
            leaseDescriptor = descriptor
            let payloadURL = directoryURL.appendingPathComponent(payloadFilename, isDirectory: false)
            let payloadDescriptor = try createLockedFile(at: payloadURL, flags: O_WRONLY)
            let owner = ArtifactOwner(
                directoryURL: directoryURL,
                payloadURL: payloadURL,
                leaseDescriptor: descriptor
            )
            leaseDescriptor = nil
            return Destination(
                owner: owner,
                handle: FileHandle(fileDescriptor: payloadDescriptor, closeOnDealloc: true)
            )
        } catch {
            if let leaseDescriptor {
                _ = flock(leaseDescriptor, LOCK_UN)
                _ = Darwin.close(leaseDescriptor)
            }
            try? FileManager.default.removeItem(at: directoryURL)
            throw MediaGenerationClientError.invalidVideoContent
        }
    }

    private static func secureDirectory(_ url: URL) throws {
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CocoaError(.fileWriteUnknown) }
        defer { _ = Darwin.close(descriptor) }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFDIR,
              status.st_uid == geteuid(),
              fchmod(descriptor, directoryMode) == 0 else {
            throw CocoaError(.fileWriteUnknown)
        }
    }

    private static func createLockedFile(at url: URL, flags: Int32) throws -> Int32 {
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                flags | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
                fileMode
            )
        }
        guard descriptor >= 0 else { throw CocoaError(.fileWriteUnknown) }
        var shouldClose = true
        defer {
            if shouldClose { _ = Darwin.close(descriptor) }
        }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              isOwnedSingleLinkRegularFile(status),
              fchmod(descriptor, fileMode) == 0 else {
            throw CocoaError(.fileWriteUnknown)
        }
        if flags == O_RDWR {
            while flock(descriptor, LOCK_EX) != 0 {
                guard errno == EINTR else { throw CocoaError(.fileWriteUnknown) }
            }
        }
        shouldClose = false
        return descriptor
    }
}
