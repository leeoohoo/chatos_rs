import Darwin
import Foundation

struct GlobalUtilityContextSnapshot: Codable, Equatable, Sendable {
    let isEnglish: Bool
    let projects: [GlobalUtilitySearchItem]
    let contacts: [GlobalUtilitySearchItem]
}

struct GlobalUtilityContextSnapshotInput: Equatable, Sendable {
    let isEnglish: Bool
    let projects: [ResourceItem]
    let contacts: [ResourceItem]

    func snapshot() -> GlobalUtilityContextSnapshot {
        .init(
            isEnglish: isEnglish,
            projects: projects.map {
                .init(id: $0.id, title: $0.title, subtitle: $0.subtitle)
            },
            contacts: contacts.map {
                .init(id: $0.id, title: $0.title, subtitle: $0.subtitle)
            }
        )
    }
}

enum GlobalUtilityContextSnapshotStore {
    static let maximumBytes = 4 * 1_024 * 1_024
    private static let chunkSize = 256 * 1_024

    static func write(_ input: GlobalUtilityContextSnapshotInput, to url: URL) async -> Bool {
        do {
            return try await AppCancellableDetachedWork.run(priority: .utility) {
                let data = try JSONEncoder().encode(input.snapshot())
                guard !data.isEmpty, data.count <= maximumBytes else { return false }
                try writeAtomically(data, to: url)
                return true
            }
        } catch {
            return false
        }
    }

    static func read(from url: URL) async -> GlobalUtilityContextSnapshot? {
        do {
            return try await AppCancellableDetachedWork.run(priority: .utility) {
                let data = try readBounded(from: url)
                return try JSONDecoder().decode(GlobalUtilityContextSnapshot.self, from: data)
            }
        } catch {
            return nil
        }
    }

    private nonisolated static func writeAtomically(_ data: Data, to url: URL) throws {
        guard url.isFileURL else { throw CocoaError(.fileWriteUnsupportedScheme) }
        try Task.checkCancellation()
        let directory = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        let temporaryURL = directory.appendingPathComponent(
            ".chatos-global-context-\(UUID().uuidString).tmp"
        )
        let descriptor = temporaryURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
                mode_t(S_IRUSR | S_IWUSR)
            )
        }
        guard descriptor >= 0 else { throw CocoaError(.fileWriteUnknown) }
        var installed = false
        defer {
            Darwin.close(descriptor)
            if !installed { try? FileManager.default.removeItem(at: temporaryURL) }
        }

        try data.withUnsafeBytes { bytes in
            var written = 0
            while written < bytes.count {
                try Task.checkCancellation()
                let count = Darwin.write(
                    descriptor,
                    bytes.baseAddress?.advanced(by: written),
                    bytes.count - written
                )
                if count < 0, errno == EINTR { continue }
                guard count > 0 else { throw CocoaError(.fileWriteUnknown) }
                written += count
            }
        }
        try Task.checkCancellation()
        guard fsync(descriptor) == 0 else { throw CocoaError(.fileWriteUnknown) }
        try Task.checkCancellation()
        let result: Int32 = temporaryURL.withUnsafeFileSystemRepresentation { temporaryPath in
            url.withUnsafeFileSystemRepresentation { destinationPath in
                guard let temporaryPath, let destinationPath else { return Int32(-1) }
                return Darwin.rename(temporaryPath, destinationPath)
            }
        }
        guard result == 0 else { throw CocoaError(.fileWriteUnknown) }
        installed = true
    }

    private nonisolated static func readBounded(from url: URL) throws -> Data {
        guard url.isFileURL else { throw CocoaError(.fileReadUnsupportedScheme) }
        try Task.checkCancellation()
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CocoaError(.fileReadNoSuchFile) }
        defer { Darwin.close(descriptor) }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_size > 0,
              status.st_size <= maximumBytes else {
            throw CocoaError(.fileReadCorruptFile)
        }

        var data = Data()
        data.reserveCapacity(Int(status.st_size))
        var buffer = [UInt8](repeating: 0, count: chunkSize)
        while data.count <= maximumBytes {
            try Task.checkCancellation()
            let count = buffer.withUnsafeMutableBytes { bytes in
                Darwin.read(descriptor, bytes.baseAddress, bytes.count)
            }
            if count < 0, errno == EINTR { continue }
            guard count >= 0 else { throw CocoaError(.fileReadUnknown) }
            if count == 0 { break }
            guard data.count <= maximumBytes - count else {
                throw CocoaError(.fileReadTooLarge)
            }
            data.append(buffer, count: count)
        }
        guard data.count == status.st_size else { throw CocoaError(.fileReadCorruptFile) }
        return data
    }
}

actor GlobalUtilityContextSnapshotPersistence {
    typealias Writer = @Sendable (GlobalUtilityContextSnapshotInput, URL) async -> Bool

    private let url: URL
    private let writer: Writer
    private let didPersist: @Sendable () -> Void
    private var pending: GlobalUtilityContextSnapshotInput?
    private var lastPersisted: GlobalUtilityContextSnapshotInput?
    private var isDraining = false
    private var isActive = true
    private var drainTask: Task<Void, Never>?

    init(
        url: URL,
        writer: @escaping Writer = { snapshot, url in
            await GlobalUtilityContextSnapshotStore.write(snapshot, to: url)
        },
        didPersist: @escaping @Sendable () -> Void
    ) {
        self.url = url
        self.writer = writer
        self.didPersist = didPersist
    }

    func submit(_ snapshot: GlobalUtilityContextSnapshotInput) {
        guard isActive else { return }
        pending = snapshot
        guard !isDraining else { return }
        isDraining = true
        drainTask = Task { [weak self] in
            await self?.drain()
        }
    }

    func stop() {
        isActive = false
        pending = nil
        drainTask?.cancel()
        drainTask = nil
    }

    private func drain() async {
        while isActive, let snapshot = pending {
            pending = nil
            if snapshot == lastPersisted { continue }
            let persisted = await writer(snapshot, url)
            guard isActive else { break }
            if persisted {
                lastPersisted = snapshot
                didPersist()
            }
        }
        isDraining = false
        drainTask = nil
    }
}
