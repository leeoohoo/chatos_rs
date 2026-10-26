import ChatOSCodeReadWorkerProtocol
import CryptoKit
import Darwin
import Foundation

struct ArtifactWorkerOperations {
    private static let maximumBytes = 2 * 1_024 * 1_024
    private static let minimumFreeBytesAfterWrite: UInt64 = 256 * 1_024 * 1_024

    let root: URL
    let rootIdentity: CodeReadWorkerFileIdentity
    let deadlineUnixMilliseconds: Int64

    func execute(
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        let sourcePath = try components(requiredString(arguments, "source_path"))
        let destinationPath = try components(requiredString(arguments, "destination_path"))
        let expectedBytes = try expectedByteCount(arguments)
        let expectedSHA256 = try expectedDigest(arguments)
        switch operation {
        case "artifact_stage_v1":
            let token = try requiredString(arguments, "token").lowercased()
            guard let uuid = UUID(uuidString: token), uuid.uuidString.lowercased() == token,
                  destinationPath.last == "\(token).staged" else {
                throw CodeReadWorkerCoreError.invalidRequest("artifact staging token is invalid")
            }
            try copy(
                sourcePath: sourcePath,
                destinationPath: destinationPath,
                expectedBytes: expectedBytes,
                expectedSHA256: expectedSHA256,
                requireSingleSourceLink: true
            )
            return .object([
                "token": .string(token),
                "relative_path": .string(destinationPath.joined(separator: "/")),
            ])
        case "artifact_restore_v1":
            try copy(
                sourcePath: sourcePath,
                destinationPath: destinationPath,
                expectedBytes: expectedBytes,
                expectedSHA256: expectedSHA256,
                requireSingleSourceLink: false
            )
            return .object([
                "relative_path": .string(destinationPath.joined(separator: "/")),
                "byte_count": .number(Double(expectedBytes)),
            ])
        default:
            throw CodeReadWorkerCoreError.unsupportedTool(operation)
        }
    }

    private func copy(
        sourcePath: [String],
        destinationPath: [String],
        expectedBytes: Int64,
        expectedSHA256: String,
        requireSingleSourceLink: Bool
    ) throws {
        try checkDeadline()
        let rootDescriptor = try openRoot()
        defer { _ = Darwin.close(rootDescriptor) }
        let sourceParent = try openDirectoryChain(
            Array(sourcePath.dropLast()),
            from: rootDescriptor,
            create: false
        )
        defer { if sourceParent != rootDescriptor { _ = Darwin.close(sourceParent) } }
        let source = sourcePath.last!.withCString {
            Darwin.openat(sourceParent, $0, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard source >= 0 else { throw CodeReadWorkerCoreError.artifactInvalid }
        defer { _ = Darwin.close(source) }

        var before = stat()
        guard Darwin.fstat(source, &before) == 0,
              (before.st_mode & S_IFMT) == S_IFREG,
              before.st_uid == geteuid(),
              (!requireSingleSourceLink || before.st_nlink == 1),
              before.st_size == expectedBytes else {
            throw CodeReadWorkerCoreError.artifactInvalid
        }

        let destinationParent = try openDirectoryChain(
            Array(destinationPath.dropLast()),
            from: rootDescriptor,
            create: true
        )
        defer { if destinationParent != rootDescriptor { _ = Darwin.close(destinationParent) } }
        try ensureCapacity(destinationParent, writeBytes: UInt64(expectedBytes))
        let leaf = destinationPath.last!
        let temporary = ".chatos-artifact-copy-\(UUID().uuidString.lowercased()).tmp"
        let destination = temporary.withCString {
            Darwin.openat(
                destinationParent,
                $0,
                O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
                0o600
            )
        }
        guard destination >= 0 else { throw CodeReadWorkerCoreError.artifactWriteFailed }
        var temporaryPresent = true
        defer {
            _ = Darwin.close(destination)
            if temporaryPresent {
                temporary.withCString { _ = Darwin.unlinkat(destinationParent, $0, 0) }
            }
        }

        var hasher = SHA256()
        var total: Int64 = 0
        var buffer = [UInt8](repeating: 0, count: 256 * 1_024)
        while total < expectedBytes {
            try checkDeadline()
            let requested = min(buffer.count, Int(expectedBytes - total))
            let count = buffer.withUnsafeMutableBytes {
                Darwin.read(source, $0.baseAddress, requested)
            }
            if count < 0, errno == EINTR { continue }
            guard count > 0 else { throw CodeReadWorkerCoreError.artifactInvalid }
            buffer.withUnsafeBytes {
                hasher.update(bufferPointer: UnsafeRawBufferPointer(rebasing: $0.prefix(count)))
            }
            try writeAll(buffer, count: count, descriptor: destination)
            total += Int64(count)
        }
        var trailingByte: UInt8 = 0
        while true {
            let count = Darwin.read(source, &trailingByte, 1)
            if count == 0 { break }
            if count < 0, errno == EINTR { continue }
            throw CodeReadWorkerCoreError.artifactInvalid
        }
        var after = stat()
        let digest = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        guard Darwin.fstat(source, &after) == 0,
              sameIdentity(before, after),
              digest == expectedSHA256 else {
            throw CodeReadWorkerCoreError.artifactInvalid
        }
        try checkDeadline()
        guard Darwin.fchmod(destination, 0o600) == 0,
              Darwin.fsync(destination) == 0 else {
            throw CodeReadWorkerCoreError.artifactWriteFailed
        }
        let linked = temporary.withCString { temporaryPointer in
            leaf.withCString { leafPointer in
                Darwin.linkat(destinationParent, temporaryPointer, destinationParent, leafPointer, 0)
            }
        }
        if linked != 0 {
            guard errno == EEXIST,
                  try existingFileMatches(
                    leaf,
                    in: destinationParent,
                    expectedBytes: expectedBytes,
                    expectedSHA256: expectedSHA256
                  ) else {
                throw CodeReadWorkerCoreError.artifactWriteFailed
            }
        }
        guard temporary.withCString({ Darwin.unlinkat(destinationParent, $0, 0) }) == 0 else {
            if linked == 0 { leaf.withCString { _ = Darwin.unlinkat(destinationParent, $0, 0) } }
            throw CodeReadWorkerCoreError.artifactWriteFailed
        }
        temporaryPresent = false
        guard Darwin.fsync(destinationParent) == 0 else {
            if linked == 0 { leaf.withCString { _ = Darwin.unlinkat(destinationParent, $0, 0) } }
            throw CodeReadWorkerCoreError.artifactWriteFailed
        }
    }

    private func existingFileMatches(
        _ name: String,
        in directory: Int32,
        expectedBytes: Int64,
        expectedSHA256: String
    ) throws -> Bool {
        let descriptor = name.withCString {
            Darwin.openat(directory, $0, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { return false }
        defer { _ = Darwin.close(descriptor) }
        var before = stat()
        guard Darwin.fstat(descriptor, &before) == 0,
              (before.st_mode & S_IFMT) == S_IFREG,
              before.st_uid == geteuid(), before.st_size == expectedBytes else { return false }
        var hasher = SHA256()
        var total: Int64 = 0
        var buffer = [UInt8](repeating: 0, count: 256 * 1_024)
        while total < expectedBytes {
            try checkDeadline()
            let requested = min(buffer.count, Int(expectedBytes - total))
            let count = buffer.withUnsafeMutableBytes {
                Darwin.read(descriptor, $0.baseAddress, requested)
            }
            if count < 0, errno == EINTR { continue }
            guard count > 0 else { return false }
            buffer.withUnsafeBytes {
                hasher.update(bufferPointer: UnsafeRawBufferPointer(rebasing: $0.prefix(count)))
            }
            total += Int64(count)
        }
        var trailingByte: UInt8 = 0
        while true {
            let count = Darwin.read(descriptor, &trailingByte, 1)
            if count == 0 { break }
            if count < 0, errno == EINTR { continue }
            return false
        }
        var after = stat()
        return Darwin.fstat(descriptor, &after) == 0
            && sameIdentity(before, after)
            && hasher.finalize().map { String(format: "%02x", $0) }.joined() == expectedSHA256
    }

    private func openRoot() throws -> Int32 {
        let descriptor = root.path.withCString {
            Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CodeReadWorkerCoreError.artifactWriteFailed }
        var status = stat()
        guard Darwin.fstat(descriptor, &status) == 0,
              UInt64(status.st_dev) == rootIdentity.device,
              UInt64(status.st_ino) == rootIdentity.inode else {
            _ = Darwin.close(descriptor)
            throw CodeReadWorkerCoreError.invalidRequest("artifact root identity changed")
        }
        return descriptor
    }

    private func openDirectoryChain(
        _ path: [String],
        from root: Int32,
        create: Bool
    ) throws -> Int32 {
        var current = root
        var ownsCurrent = false
        do {
            for component in path {
                try checkDeadline()
                if create, component.withCString({ Darwin.mkdirat(current, $0, 0o700) }) != 0,
                   errno != EEXIST {
                    throw CodeReadWorkerCoreError.artifactWriteFailed
                }
                let next = component.withCString {
                    Darwin.openat(current, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
                }
                guard next >= 0 else {
                    throw create
                        ? CodeReadWorkerCoreError.artifactWriteFailed
                        : CodeReadWorkerCoreError.artifactInvalid
                }
                var status = stat()
                guard Darwin.fstat(next, &status) == 0,
                      (status.st_mode & S_IFMT) == S_IFDIR,
                      status.st_uid == geteuid() else {
                    _ = Darwin.close(next)
                    throw CodeReadWorkerCoreError.artifactWriteFailed
                }
                if create, Darwin.fchmod(next, 0o700) != 0 {
                    _ = Darwin.close(next)
                    throw CodeReadWorkerCoreError.artifactWriteFailed
                }
                if ownsCurrent { _ = Darwin.close(current) }
                current = next
                ownsCurrent = true
            }
            return current
        } catch {
            if ownsCurrent { _ = Darwin.close(current) }
            throw error
        }
    }

    private func writeAll(_ buffer: [UInt8], count: Int, descriptor: Int32) throws {
        var offset = 0
        while offset < count {
            try checkDeadline()
            let written = buffer.withUnsafeBytes {
                Darwin.write(descriptor, $0.baseAddress?.advanced(by: offset), count - offset)
            }
            if written < 0, errno == EINTR { continue }
            guard written > 0 else { throw CodeReadWorkerCoreError.artifactWriteFailed }
            offset += written
        }
    }

    private func ensureCapacity(_ descriptor: Int32, writeBytes: UInt64) throws {
        var status = statfs()
        guard Darwin.fstatfs(descriptor, &status) == 0,
              status.f_bsize > 0, status.f_bavail >= 0 else {
            throw CodeReadWorkerCoreError.artifactWriteFailed
        }
        let blockSize = UInt64(status.f_bsize)
        let availableBlocks = UInt64(status.f_bavail)
        let (available, overflow) = availableBlocks.multipliedReportingOverflow(by: blockSize)
        let (required, requiredOverflow) = writeBytes.addingReportingOverflow(
            Self.minimumFreeBytesAfterWrite
        )
        guard !overflow, !requiredOverflow, available >= required else {
            throw CodeReadWorkerCoreError.artifactWriteFailed
        }
    }

    private func components(_ path: String) throws -> [String] {
        guard !path.hasPrefix("/"), !path.contains("\0") else {
            throw CodeReadWorkerCoreError.pathOutsideProject
        }
        var result: [String] = []
        for raw in path.split(separator: "/", omittingEmptySubsequences: true) {
            let value = String(raw)
            if value == "." { continue }
            guard value != "..", !value.contains("\\") else {
                throw CodeReadWorkerCoreError.pathOutsideProject
            }
            result.append(value)
        }
        guard !result.isEmpty else { throw CodeReadWorkerCoreError.rootMutation }
        return result
    }

    private func expectedByteCount(_ arguments: [String: CodeReadWorkerJSONValue]) throws -> Int64 {
        guard case let .number(value)? = arguments["byte_size"], value.isFinite,
              value.rounded() == value, value > 0, value <= Double(Self.maximumBytes) else {
            throw CodeReadWorkerCoreError.invalidRequest("artifact byte size is invalid")
        }
        return Int64(value)
    }

    private func expectedDigest(_ arguments: [String: CodeReadWorkerJSONValue]) throws -> String {
        let value = try requiredString(arguments, "sha256").lowercased()
        guard value.utf8.count == 64,
              value.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else {
            throw CodeReadWorkerCoreError.invalidRequest("artifact digest is invalid")
        }
        return value
    }

    private func requiredString(
        _ arguments: [String: CodeReadWorkerJSONValue],
        _ key: String
    ) throws -> String {
        guard case let .string(value)? = arguments[key], !value.isEmpty else {
            throw CodeReadWorkerCoreError.invalidRequest("missing artifact field: \(key)")
        }
        return value
    }

    private func sameIdentity(_ lhs: stat, _ rhs: stat) -> Bool {
        lhs.st_dev == rhs.st_dev && lhs.st_ino == rhs.st_ino && lhs.st_size == rhs.st_size
            && lhs.st_mtimespec.tv_sec == rhs.st_mtimespec.tv_sec
            && lhs.st_mtimespec.tv_nsec == rhs.st_mtimespec.tv_nsec
            && lhs.st_ctimespec.tv_sec == rhs.st_ctimespec.tv_sec
            && lhs.st_ctimespec.tv_nsec == rhs.st_ctimespec.tv_nsec
    }

    private func checkDeadline() throws {
        guard Int64(Date().timeIntervalSince1970 * 1_000) <= deadlineUnixMilliseconds else {
            throw CodeReadWorkerCoreError.deadlineExceeded
        }
    }
}
