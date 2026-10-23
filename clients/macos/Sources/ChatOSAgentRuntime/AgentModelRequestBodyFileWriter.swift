import Darwin
import Foundation

enum AgentModelRequestBodyFileWriter {
    static let maximumSourceBytes = AgentAttachmentDataLoader.maximumBytes
    static let maximumRequestBytes = 64 * 1_024 * 1_024
    private static let sourceChunkBytes = 48 * 1_024
    private static let minimumFreeBytes = 64 * 1_024 * 1_024
    private static let artifactDirectoryName = ".chatos-agent-model-requests"
    private static let artifactPrefix = "request-"
    private static let artifactSuffix = ".json"
    private static let orphanRetention: TimeInterval = 24 * 60 * 60
    private static let maximumCleanupCandidates = 64

    struct Source: Sendable, Equatable {
        let placeholder: String
        let url: URL
        let mimeType: String
    }

    private struct OpenedSource {
        let descriptor: Int32
        let initialStatus: stat
        let byteCount: Int
        let markerRange: Range<Data.Index>
        let dataURLPrefix: Data
        let encodedByteCount: Int
    }

    static func write(
        payload: [String: Any],
        sources: [Source],
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil
    ) throws -> AgentModelRequestBodyFile {
        guard !sources.isEmpty,
              Set(sources.map(\.placeholder)).count == sources.count else {
            throw AgentRuntimeError.invalidAttachment
        }
        let encodedPayload = try JSONSerialization.data(withJSONObject: payload)
        var openedSources: [OpenedSource] = []
        openedSources.reserveCapacity(sources.count)
        do {
            for source in sources {
                let marker = Data("\"\(source.placeholder)\"".utf8)
                guard let markerRange = encodedPayload.range(of: marker),
                      encodedPayload[markerRange.upperBound...].range(of: marker) == nil,
                      isSafeMIMEType(source.mimeType) else {
                    throw AgentRuntimeError.invalidAttachment
                }
                let opened = try openSource(source.url)
                openedSources.append(.init(
                    descriptor: opened.descriptor,
                    initialStatus: opened.status,
                    byteCount: opened.byteCount,
                    markerRange: markerRange,
                    dataURLPrefix: Data(
                        "\"data:\(source.mimeType.lowercased());base64,".utf8
                    ),
                    encodedByteCount: 4 * ((opened.byteCount + 2) / 3)
                ))
            }
        } catch {
            openedSources.forEach { Darwin.close($0.descriptor) }
            throw error
        }
        defer { openedSources.forEach { Darwin.close($0.descriptor) } }
        openedSources.sort { $0.markerRange.lowerBound < $1.markerRange.lowerBound }

        var requestByteCount = encodedPayload.count
        for source in openedSources {
            let replacementByteCount = source.dataURLPrefix.count
                + source.encodedByteCount
                + 1
            let (withoutMarker, subtractOverflow) = requestByteCount
                .subtractingReportingOverflow(source.markerRange.count)
            let (withReplacement, addOverflow) = withoutMarker
                .addingReportingOverflow(replacementByteCount)
            guard !subtractOverflow, !addOverflow else {
                throw AgentRuntimeError.invalidAttachment
            }
            requestByteCount = withReplacement
        }
        guard requestByteCount > 0,
              requestByteCount <= maximumRequestBytes else {
            throw AgentRuntimeError.invalidAttachment
        }
        let artifactDirectory = try prepareArtifactDirectory(in: temporaryDirectory)
        _ = reconcileOrphans(
            in: temporaryDirectory,
            olderThan: Date().addingTimeInterval(-orphanRetention),
            maximumCandidates: maximumCleanupCandidates
        )
        try ensureCapacity(
            in: artifactDirectory,
            requestByteCount: requestByteCount,
            availableCapacityOverride: availableCapacityOverride
        )

        let destinationURL = artifactDirectory.appendingPathComponent(
            "\(artifactPrefix)\(UUID().uuidString.lowercased())\(artifactSuffix)"
        )
        let destination = destinationURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
                mode_t(0o600)
            )
        }
        guard destination >= 0,
              Darwin.fchmod(destination, mode_t(0o600)) == 0 else {
            if destination >= 0 { Darwin.close(destination) }
            _ = destinationURL.withUnsafeFileSystemRepresentation { path in
                path.map(Darwin.unlink) ?? -1
            }
            throw AgentRuntimeError.invalidAttachment
        }
        var completed = false
        defer {
            Darwin.close(destination)
            if !completed {
                _ = destinationURL.withUnsafeFileSystemRepresentation { path in
                    path.map(Darwin.unlink) ?? -1
                }
            }
        }

        var payloadCursor = encodedPayload.startIndex
        for source in openedSources {
            try writeAll(
                encodedPayload[payloadCursor..<source.markerRange.lowerBound],
                descriptor: destination
            )
            try writeAll(source.dataURLPrefix, descriptor: destination)
            var remaining = source.byteCount
            while remaining > 0 {
                try Task.checkCancellation()
                let count = min(sourceChunkBytes, remaining)
                let chunk = try readExactly(source.descriptor, count: count)
                try writeAll(chunk.base64EncodedData(), descriptor: destination)
                remaining -= count
            }
            try verifyUnchanged(source)
            try writeAll(Data("\"".utf8), descriptor: destination)
            payloadCursor = source.markerRange.upperBound
        }
        try writeAll(encodedPayload[payloadCursor...], descriptor: destination)
        guard Darwin.fsync(destination) == 0 else {
            throw AgentRuntimeError.invalidAttachment
        }
        completed = true
        return .init(url: destinationURL, byteCount: requestByteCount)
    }

    @discardableResult
    static func reconcileOrphans(
        in temporaryDirectory: URL,
        olderThan cutoff: Date,
        maximumCandidates: Int = maximumCleanupCandidates
    ) -> Int {
        guard maximumCandidates > 0 else { return 0 }
        let artifactDirectory = temporaryDirectory.appendingPathComponent(
            artifactDirectoryName,
            isDirectory: true
        )
        guard let directoryStatus = secureStatus(artifactDirectory),
              (directoryStatus.st_mode & S_IFMT) == S_IFDIR,
              directoryStatus.st_uid == geteuid(),
              directoryStatus.st_mode & 0o077 == 0,
              let children = try? FileManager.default.contentsOfDirectory(
                at: artifactDirectory,
                includingPropertiesForKeys: nil
              ) else { return 0 }
        let candidates = children.compactMap { url -> (URL, stat)? in
            guard isArtifactName(url.lastPathComponent),
                  let status = secureStatus(url),
                  (status.st_mode & S_IFMT) == S_IFREG,
                  status.st_uid == geteuid(),
                  status.st_nlink == 1,
                  modificationDate(status) < cutoff else { return nil }
            return (url, status)
        }
        .sorted { $0.0.lastPathComponent < $1.0.lastPathComponent }
        .prefix(maximumCandidates)

        var removed = 0
        for (url, originalStatus) in candidates {
            guard let current = secureStatus(url),
                  current.st_dev == originalStatus.st_dev,
                  current.st_ino == originalStatus.st_ino,
                  current.st_nlink == 1 else { continue }
            let result = url.withUnsafeFileSystemRepresentation { path in
                path.map(Darwin.unlink) ?? -1
            }
            if result == 0 { removed += 1 }
        }
        return removed
    }

    private static func openSource(_ url: URL) throws -> (
        descriptor: Int32,
        status: stat,
        byteCount: Int
    ) {
        guard url.isFileURL else { throw AgentRuntimeError.invalidAttachment }
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw AgentRuntimeError.invalidAttachment }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_uid == geteuid(),
              status.st_nlink == 1,
              status.st_size > 0,
              status.st_size <= maximumSourceBytes else {
            Darwin.close(descriptor)
            throw AgentRuntimeError.invalidAttachment
        }
        return (descriptor, status, Int(status.st_size))
    }

    private static func prepareArtifactDirectory(in root: URL) throws -> URL {
        let directory = root.appendingPathComponent(
            artifactDirectoryName,
            isDirectory: true
        )
        let result = directory.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            if Darwin.mkdir(path, mode_t(0o700)) == 0 || errno == EEXIST { return 0 }
            return -1
        }
        guard result == 0 else { throw AgentRuntimeError.invalidAttachment }
        let descriptor = directory.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw AgentRuntimeError.invalidAttachment }
        defer { Darwin.close(descriptor) }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFDIR,
              status.st_uid == geteuid(),
              fchmod(descriptor, mode_t(0o700)) == 0 else {
            throw AgentRuntimeError.invalidAttachment
        }
        return directory
    }

    private static func readExactly(_ descriptor: Int32, count: Int) throws -> Data {
        var data = Data(count: count)
        var offset = 0
        while offset < count {
            try Task.checkCancellation()
            let readCount = data.withUnsafeMutableBytes { bytes in
                Darwin.read(
                    descriptor,
                    bytes.baseAddress?.advanced(by: offset),
                    count - offset
                )
            }
            if readCount < 0, errno == EINTR { continue }
            guard readCount > 0 else { throw AgentRuntimeError.invalidAttachment }
            offset += readCount
        }
        return data
    }

    private static func writeAll<C: DataProtocol>(
        _ data: C,
        descriptor: Int32
    ) throws {
        for region in data.regions {
            try region.withUnsafeBytes { bytes in
                var offset = 0
                while offset < bytes.count {
                    try Task.checkCancellation()
                    let written = Darwin.write(
                        descriptor,
                        bytes.baseAddress?.advanced(by: offset),
                        bytes.count - offset
                    )
                    if written < 0, errno == EINTR { continue }
                    guard written > 0 else { throw AgentRuntimeError.invalidAttachment }
                    offset += written
                }
            }
        }
    }

    private static func verifyUnchanged(_ source: OpenedSource) throws {
        var finalStatus = stat()
        let initial = source.initialStatus
        guard fstat(source.descriptor, &finalStatus) == 0,
              initial.st_dev == finalStatus.st_dev,
              initial.st_ino == finalStatus.st_ino,
              initial.st_nlink == finalStatus.st_nlink,
              initial.st_size == finalStatus.st_size,
              initial.st_mtimespec.tv_sec == finalStatus.st_mtimespec.tv_sec,
              initial.st_mtimespec.tv_nsec == finalStatus.st_mtimespec.tv_nsec,
              initial.st_ctimespec.tv_sec == finalStatus.st_ctimespec.tv_sec,
              initial.st_ctimespec.tv_nsec == finalStatus.st_ctimespec.tv_nsec else {
            throw AgentRuntimeError.invalidAttachment
        }
    }

    private static func ensureCapacity(
        in directory: URL,
        requestByteCount: Int,
        availableCapacityOverride: Int64?
    ) throws {
        let available: Int64
        if let availableCapacityOverride {
            available = availableCapacityOverride
        } else {
            guard let measured = try directory.resourceValues(
                forKeys: [.volumeAvailableCapacityForImportantUsageKey]
            ).volumeAvailableCapacityForImportantUsage else {
                throw AgentRuntimeError.invalidAttachment
            }
            available = measured
        }
        let required = Int64(requestByteCount) + Int64(minimumFreeBytes)
        guard available >= required else { throw AgentRuntimeError.invalidAttachment }
    }

    private static func isSafeMIMEType(_ value: String) -> Bool {
        let bytes = value.lowercased().utf8
        let slashCount = bytes.reduce(into: 0) { count, byte in
            if byte == 47 { count += 1 }
        }
        guard !bytes.isEmpty, bytes.count <= 127,
              slashCount == 1 else { return false }
        return bytes.allSatisfy { byte in
            (97...122).contains(byte)
                || (48...57).contains(byte)
                || [33, 35, 36, 38, 43, 45, 46, 47, 94, 95].contains(byte)
        }
    }

    private static func isArtifactName(_ name: String) -> Bool {
        guard name.hasPrefix(artifactPrefix), name.hasSuffix(artifactSuffix) else {
            return false
        }
        let start = name.index(name.startIndex, offsetBy: artifactPrefix.count)
        let end = name.index(name.endIndex, offsetBy: -artifactSuffix.count)
        let token = String(name[start..<end])
        return token.count == 36
            && token == token.lowercased()
            && UUID(uuidString: token)?.uuidString.lowercased() == token
    }

    private static func secureStatus(_ url: URL) -> stat? {
        var status = stat()
        let result = url.withUnsafeFileSystemRepresentation { path in
            path.map { Darwin.lstat($0, &status) } ?? -1
        }
        return result == 0 ? status : nil
    }

    private static func modificationDate(_ status: stat) -> Date {
        Date(
            timeIntervalSince1970: TimeInterval(status.st_mtimespec.tv_sec)
                + TimeInterval(status.st_mtimespec.tv_nsec) / 1_000_000_000
        )
    }
}
