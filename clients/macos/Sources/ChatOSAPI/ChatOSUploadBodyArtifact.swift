import Darwin
import Foundation

/// Lease-aware ownership for generated upload bodies. Request writers return only the payload
/// URL, while this owner keeps the directory live until every request copy has finished with it.
enum ChatOSUploadBodyArtifact {
    enum Kind: CaseIterable {
        case videoJSON
        case imageMultipart

        var prefix: String {
            switch self {
            case .videoJSON: ".chatos-video-request-"
            case .imageMultipart: ".chatos-image-request-"
            }
        }

        var payloadFilename: String {
            switch self {
            case .videoJSON: "request.json"
            case .imageMultipart: "request.multipart"
            }
        }
    }

    final class Owner: @unchecked Sendable {
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

            try? FileManager.default.removeItem(at: directoryURL)
            if let descriptor {
                _ = flock(descriptor, LOCK_UN)
                _ = Darwin.close(descriptor)
            }
        }
    }

    struct Destination {
        let owner: Owner
        let handle: FileHandle
    }

    private static let leaseFilename = ".lease"
    private static let fileMode = mode_t(0o600)
    private static let directoryMode = mode_t(0o700)
    private static let orphanRetention: TimeInterval = 24 * 60 * 60
    private static let maximumCleanupCandidates = 64

    static func performMaintenance(in directory: URL) {
        _ = reconcileOrphans(
            in: directory,
            olderThan: Date().addingTimeInterval(-orphanRetention),
            maximumCandidates: maximumCleanupCandidates
        )
    }

    static func makeDestination(kind: Kind, in root: URL) throws -> Destination {
        let directory = root.appendingPathComponent(
            "\(kind.prefix)\(UUID().uuidString.lowercased())",
            isDirectory: true
        )
        let created = directory.withUnsafeFileSystemRepresentation { path -> Bool in
            path.map { Darwin.mkdir($0, directoryMode) == 0 } ?? false
        }
        guard created else { throw CocoaError(.fileWriteUnknown) }

        var leaseDescriptor: Int32?
        do {
            try secureDirectory(directory)
            let lease = directory.appendingPathComponent(leaseFilename)
            let descriptor = try createFile(at: lease, flags: O_RDWR, lock: true)
            leaseDescriptor = descriptor
            let payload = directory.appendingPathComponent(kind.payloadFilename)
            let payloadDescriptor = try createFile(at: payload, flags: O_WRONLY, lock: false)
            let owner = Owner(
                directoryURL: directory,
                payloadURL: payload,
                leaseDescriptor: descriptor
            )
            leaseDescriptor = nil
            return .init(
                owner: owner,
                handle: FileHandle(fileDescriptor: payloadDescriptor, closeOnDealloc: true)
            )
        } catch {
            if let leaseDescriptor {
                _ = flock(leaseDescriptor, LOCK_UN)
                _ = Darwin.close(leaseDescriptor)
            }
            try? FileManager.default.removeItem(at: directory)
            throw error
        }
    }

    @discardableResult
    static func reconcileOrphans(
        in root: URL,
        olderThan cutoff: Date,
        maximumCandidates: Int = maximumCleanupCandidates
    ) -> Int {
        guard maximumCandidates > 0,
              let children = try? FileManager.default.contentsOfDirectory(
                at: root, includingPropertiesForKeys: nil
              ) else { return 0 }
        let candidates = children.compactMap { url -> (URL, Kind, stat)? in
            guard let kind = kind(for: url.lastPathComponent),
                  let status = secureStatus(url),
                  (status.st_mode & S_IFMT) == S_IFDIR,
                  status.st_uid == geteuid(),
                  modificationDate(status) < cutoff else { return nil }
            return (url, kind, status)
        }
        .sorted { $0.0.lastPathComponent < $1.0.lastPathComponent }
        .prefix(maximumCandidates)

        var removed = 0
        for (directory, kind, status) in candidates {
            if reconcile(directory, kind: kind, originalDirectoryStatus: status) {
                removed += 1
            }
        }
        return removed
    }

    private static func reconcile(
        _ directory: URL,
        kind: Kind,
        originalDirectoryStatus: stat
    ) -> Bool {
        guard let children = try? FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: nil
        ) else { return false }
        let names = Set(children.map(\.lastPathComponent))
        guard names.isSubset(of: [leaseFilename, kind.payloadFilename]) else { return false }

        let lease = directory.appendingPathComponent(leaseFilename)
        guard names.contains(leaseFilename) else {
            guard names.isEmpty,
                  directoryIsUnchanged(directory, originalStatus: originalDirectoryStatus),
                  let currentChildren = try? FileManager.default.contentsOfDirectory(
                    at: directory, includingPropertiesForKeys: nil
                  ), currentChildren.isEmpty else { return false }
            return (try? FileManager.default.removeItem(at: directory)) != nil
        }
        guard let leasePathStatus = secureStatus(lease),
              isOwnedSingleLinkRegularFile(leasePathStatus) else { return false }
        if names.contains(kind.payloadFilename) {
            let payload = directory.appendingPathComponent(kind.payloadFilename)
            guard let payloadStatus = secureStatus(payload),
                  isOwnedSingleLinkRegularFile(payloadStatus) else { return false }
        }

        let descriptor = lease.withUnsafeFileSystemRepresentation { path -> Int32 in
            path.map { Darwin.open($0, O_RDWR | O_NOFOLLOW | O_CLOEXEC) } ?? -1
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
              directoryIsUnchanged(directory, originalStatus: originalDirectoryStatus),
              let lockedChildren = try? FileManager.default.contentsOfDirectory(
                at: directory, includingPropertiesForKeys: nil
              ), Set(lockedChildren.map(\.lastPathComponent)) == names,
              let lockedLeaseStatus = secureStatus(lease),
              isOwnedSingleLinkRegularFile(lockedLeaseStatus),
              lockedLeaseStatus.st_dev == descriptorStatus.st_dev,
              lockedLeaseStatus.st_ino == descriptorStatus.st_ino else { return false }
        if names.contains(kind.payloadFilename) {
            let payload = directory.appendingPathComponent(kind.payloadFilename)
            guard let payloadStatus = secureStatus(payload),
                  isOwnedSingleLinkRegularFile(payloadStatus) else { return false }
        }
        return (try? FileManager.default.removeItem(at: directory)) != nil
    }

    private static func kind(for name: String) -> Kind? {
        for kind in Kind.allCases where name.hasPrefix(kind.prefix) {
            let value = String(name.dropFirst(kind.prefix.count))
            guard value.count == 36,
                  value == value.lowercased(),
                  let id = UUID(uuidString: value),
                  id.uuidString.lowercased() == value else { return nil }
            return kind
        }
        return nil
    }

    private static func secureDirectory(_ url: URL) throws {
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            path.map { Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC) } ?? -1
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

    private static func createFile(at url: URL, flags: Int32, lock: Bool) throws -> Int32 {
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            path.map {
                Darwin.open($0, flags | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, fileMode)
            } ?? -1
        }
        guard descriptor >= 0 else { throw CocoaError(.fileWriteUnknown) }
        var shouldClose = true
        defer { if shouldClose { _ = Darwin.close(descriptor) } }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              isOwnedSingleLinkRegularFile(status),
              fchmod(descriptor, fileMode) == 0 else {
            throw CocoaError(.fileWriteUnknown)
        }
        if lock {
            while flock(descriptor, LOCK_EX) != 0 {
                guard errno == EINTR else { throw CocoaError(.fileWriteUnknown) }
            }
        }
        shouldClose = false
        return descriptor
    }

    private static func directoryIsUnchanged(_ url: URL, originalStatus: stat) -> Bool {
        guard let current = secureStatus(url),
              (current.st_mode & S_IFMT) == S_IFDIR,
              current.st_uid == geteuid() else { return false }
        return current.st_dev == originalStatus.st_dev && current.st_ino == originalStatus.st_ino
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
}
