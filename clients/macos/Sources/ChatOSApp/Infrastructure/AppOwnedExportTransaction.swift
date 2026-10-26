import Darwin
import Foundation

/// Owns one atomic export in a user-selected directory and opportunistically reclaims only
/// canonical, stale ChatOS export files in that exact directory. All mutations stay bound to the
/// opened directory descriptor so a path replacement cannot redirect an in-flight export.
final class AppOwnedExportTransaction {
    private static let minimumCleanupAgeSeconds: Int64 = 24 * 60 * 60

    let descriptor: Int32
    private let directoryURL: URL
    private let directoryIdentity: stat
    private let temporaryName: String
    private(set) var fileDescriptor: Int32
    private var installed = false

    init(destinationURL: URL, temporaryPrefix: String) throws {
        guard destinationURL.isFileURL,
              Self.validLeafName(destinationURL.lastPathComponent),
              temporaryPrefix.hasPrefix(".chatos-"),
              temporaryPrefix.hasSuffix("-") else {
            throw CocoaError(.fileWriteInvalidFileName)
        }
        let directoryURL = destinationURL.deletingLastPathComponent()
            .standardizedFileURL.resolvingSymlinksInPath()
        let descriptor = directoryURL.path.withCString {
            Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CocoaError(.fileNoSuchFile) }
        var directoryIdentity = stat()
        guard Darwin.fstat(descriptor, &directoryIdentity) == 0,
              (directoryIdentity.st_mode & S_IFMT) == S_IFDIR else {
            _ = Darwin.close(descriptor)
            throw CocoaError(.fileWriteUnknown)
        }

        Self.cleanupStaleFiles(in: descriptor, prefix: temporaryPrefix)
        let temporaryName = temporaryPrefix + UUID().uuidString.lowercased() + ".tmp"
        let fileDescriptor = temporaryName.withCString {
            Darwin.openat(
                descriptor,
                $0,
                O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
                mode_t(S_IRUSR | S_IWUSR)
            )
        }
        guard fileDescriptor >= 0 else {
            _ = Darwin.close(descriptor)
            throw CocoaError(.fileWriteUnknown)
        }
        guard flock(fileDescriptor, LOCK_EX | LOCK_NB) == 0 else {
            _ = Darwin.close(fileDescriptor)
            temporaryName.withCString { _ = Darwin.unlinkat(descriptor, $0, 0) }
            _ = Darwin.close(descriptor)
            throw CocoaError(.fileWriteUnknown)
        }

        self.descriptor = descriptor
        self.directoryURL = directoryURL
        self.directoryIdentity = directoryIdentity
        self.temporaryName = temporaryName
        self.fileDescriptor = fileDescriptor
    }

    deinit {
        if fileDescriptor >= 0 { _ = Darwin.close(fileDescriptor) }
        if !installed {
            temporaryName.withCString { _ = Darwin.unlinkat(descriptor, $0, 0) }
        }
        _ = Darwin.close(descriptor)
    }

    func install(as destinationName: String) throws {
        guard Self.validLeafName(destinationName) else {
            throw CocoaError(.fileWriteInvalidFileName)
        }
        var openedDirectory = stat()
        guard Darwin.fstat(descriptor, &openedDirectory) == 0,
              Self.sameDirectory(directoryIdentity, openedDirectory) else {
            throw CocoaError(.fileWriteUnknown)
        }
        let reopened = directoryURL.path.withCString {
            Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard reopened >= 0 else { throw CocoaError(.fileWriteUnknown) }
        defer { _ = Darwin.close(reopened) }
        var reopenedDirectory = stat()
        guard Darwin.fstat(reopened, &reopenedDirectory) == 0,
              Self.sameDirectory(directoryIdentity, reopenedDirectory) else {
            throw CocoaError(.fileWriteUnknown)
        }

        var openedFile = stat()
        var namedFile = stat()
        guard Darwin.fstat(fileDescriptor, &openedFile) == 0,
              (openedFile.st_mode & S_IFMT) == S_IFREG,
              openedFile.st_uid == geteuid(), openedFile.st_nlink == 1,
              temporaryName.withCString({
                  Darwin.fstatat(descriptor, $0, &namedFile, AT_SYMLINK_NOFOLLOW)
              }) == 0,
              Self.sameFile(openedFile, namedFile) else {
            throw CocoaError(.fileWriteUnknown)
        }
        let result = temporaryName.withCString { source in
            destinationName.withCString { destination in
                Darwin.renameat(descriptor, source, descriptor, destination)
            }
        }
        guard result == 0 else { throw CocoaError(.fileWriteUnknown) }
        installed = true
        guard Darwin.fsync(descriptor) == 0 else { throw CocoaError(.fileWriteUnknown) }
    }

    private static func cleanupStaleFiles(in directory: Int32, prefix: String) {
        guard let stream = Darwin.fdopendir(Darwin.dup(directory)) else { return }
        defer { Darwin.closedir(stream) }
        let cutoff = Int64(Date().timeIntervalSince1970) - minimumCleanupAgeSeconds
        while let entry = Darwin.readdir(stream) {
            let name = withUnsafePointer(to: &entry.pointee.d_name) {
                $0.withMemoryRebound(to: CChar.self, capacity: Int(MAXNAMLEN) + 1) {
                    String(cString: $0)
                }
            }
            guard canonicalTemporaryName(name, prefix: prefix) else { continue }
            var inspected = stat()
            guard name.withCString({
                Darwin.fstatat(directory, $0, &inspected, AT_SYMLINK_NOFOLLOW)
            }) == 0,
            (inspected.st_mode & S_IFMT) == S_IFREG,
            inspected.st_uid == geteuid(), inspected.st_nlink == 1,
            Int64(inspected.st_mtimespec.tv_sec) <= cutoff else { continue }
            let file = name.withCString {
                Darwin.openat(directory, $0, O_RDWR | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
            }
            guard file >= 0 else { continue }
            do {
                defer { _ = Darwin.close(file) }
                guard flock(file, LOCK_EX | LOCK_NB) == 0 else { continue }
                var opened = stat()
                var current = stat()
                guard Darwin.fstat(file, &opened) == 0,
                      sameFile(inspected, opened),
                      name.withCString({
                          Darwin.fstatat(directory, $0, &current, AT_SYMLINK_NOFOLLOW)
                      }) == 0,
                      sameFile(inspected, current) else { continue }
                name.withCString { _ = Darwin.unlinkat(directory, $0, 0) }
            }
        }
    }

    private static func canonicalTemporaryName(_ name: String, prefix: String) -> Bool {
        guard name.hasPrefix(prefix), name.hasSuffix(".tmp") else { return false }
        let value = String(name.dropFirst(prefix.count).dropLast(".tmp".count))
        guard let uuid = UUID(uuidString: value) else { return false }
        return uuid.uuidString.lowercased() == value
    }

    private static func validLeafName(_ name: String) -> Bool {
        !name.isEmpty && name != "." && name != ".."
            && !name.contains("/") && !name.contains("\0")
    }

    private static func sameDirectory(_ lhs: stat, _ rhs: stat) -> Bool {
        lhs.st_dev == rhs.st_dev && lhs.st_ino == rhs.st_ino
            && (rhs.st_mode & S_IFMT) == S_IFDIR
    }

    private static func sameFile(_ lhs: stat, _ rhs: stat) -> Bool {
        lhs.st_dev == rhs.st_dev && lhs.st_ino == rhs.st_ino
            && lhs.st_size == rhs.st_size
            && lhs.st_mtimespec.tv_sec == rhs.st_mtimespec.tv_sec
            && lhs.st_mtimespec.tv_nsec == rhs.st_mtimespec.tv_nsec
            && lhs.st_ctimespec.tv_sec == rhs.st_ctimespec.tv_sec
            && lhs.st_ctimespec.tv_nsec == rhs.st_ctimespec.tv_nsec
    }
}
