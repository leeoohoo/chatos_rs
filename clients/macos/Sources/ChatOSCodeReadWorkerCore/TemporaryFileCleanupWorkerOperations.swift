import ChatOSCodeReadWorkerProtocol
import Darwin
import Foundation

struct TemporaryFileCleanupWorkerOperations {
    private static let minimumAgeSeconds: Int64 = 24 * 60 * 60
    private static let maximumDepth = 4
    private static let maximumEntries = 20_000
    private static let allowedScopes = [
        "LocalAgent/Attachments": 2,
        "LocalAgent/local-agent.artifact-staging": 0,
        "AgentGroupChatAttachments": 2,
    ]

    let root: URL
    let rootIdentity: CodeReadWorkerFileIdentity
    let deadlineUnixMilliseconds: Int64

    func execute(
        arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        guard case let .array(rawScopes)? = arguments["scopes"],
              rawScopes.count == Self.allowedScopes.count else {
            throw CodeReadWorkerCoreError.invalidRequest("temporary cleanup scopes are invalid")
        }
        var seenPaths = Set<String>()
        let scopes = try rawScopes.map { value -> Scope in
            guard case let .object(fields) = value,
                  case let .string(path)? = fields["path"],
                  case let .number(depthValue)? = fields["maximum_depth"],
                  depthValue.isFinite, depthValue.rounded() == depthValue,
                  (0...Double(Self.maximumDepth)).contains(depthValue),
                  let allowedDepth = Self.allowedScopes[path],
                  Int(depthValue) == allowedDepth,
                  seenPaths.insert(path).inserted else {
                throw CodeReadWorkerCoreError.invalidRequest("temporary cleanup scope is invalid")
            }
            return .init(path: try components(path), maximumDepth: Int(depthValue))
        }
        guard seenPaths.count == Self.allowedScopes.count else {
            throw CodeReadWorkerCoreError.invalidRequest("temporary cleanup scopes are incomplete")
        }
        let minimumAge = arguments["minimum_age_seconds"].flatMap { value -> Int64? in
            guard case let .number(raw) = value, raw.isFinite, raw.rounded() == raw,
                  raw >= Double(Self.minimumAgeSeconds), raw <= Double(Int32.max) else {
                return nil
            }
            return Int64(raw)
        }
        guard let minimumAge else {
            throw CodeReadWorkerCoreError.invalidRequest("temporary cleanup age is invalid")
        }
        let now = Int64(Date().timeIntervalSince1970)
        let cutoff = now - minimumAge
        let rootDescriptor = try openRoot()
        defer { _ = Darwin.close(rootDescriptor) }
        var state = CleanupState()
        for scope in scopes {
            try checkDeadline()
            guard state.visited < Self.maximumEntries else {
                state.truncated = true
                break
            }
            guard let directory = try openExistingDirectoryChain(scope.path, from: rootDescriptor)
            else { continue }
            defer { if directory != rootDescriptor { _ = Darwin.close(directory) } }
            try scan(
                directory,
                depth: 0,
                maximumDepth: scope.maximumDepth,
                cutoffUnixSeconds: cutoff,
                state: &state
            )
        }
        return .object([
            "visited_entries": .number(Double(state.visited)),
            "eligible_entries": .number(Double(state.eligible)),
            "deleted_entries": .number(Double(state.deleted)),
            "locked_entries": .number(Double(state.locked)),
            "truncated": .bool(state.truncated),
        ])
    }

    private func scan(
        _ directory: Int32,
        depth: Int,
        maximumDepth: Int,
        cutoffUnixSeconds: Int64,
        state: inout CleanupState
    ) throws {
        guard let stream = Darwin.fdopendir(Darwin.dup(directory)) else {
            throw CodeReadWorkerCoreError.filesystemFailure("temporary cleanup directory failed")
        }
        defer { Darwin.closedir(stream) }
        while let entry = Darwin.readdir(stream) {
            try checkDeadline()
            guard state.visited < Self.maximumEntries else {
                state.truncated = true
                return
            }
            let name = withUnsafePointer(to: &entry.pointee.d_name) {
                $0.withMemoryRebound(to: CChar.self, capacity: Int(MAXNAMLEN) + 1) {
                    String(cString: $0)
                }
            }
            if name == "." || name == ".." { continue }
            state.visited += 1
            var status = stat()
            let inspected = name.withCString {
                Darwin.fstatat(directory, $0, &status, AT_SYMLINK_NOFOLLOW)
            }
            if inspected != 0 {
                if errno == ENOENT { continue }
                throw CodeReadWorkerCoreError.filesystemFailure("temporary cleanup stat failed")
            }
            if (status.st_mode & S_IFMT) == S_IFDIR, depth < maximumDepth {
                let child = name.withCString {
                    Darwin.openat(directory, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
                }
                guard child >= 0 else {
                    if errno == ENOENT || errno == ELOOP { continue }
                    throw CodeReadWorkerCoreError.filesystemFailure(
                        "temporary cleanup directory changed"
                    )
                }
                defer { _ = Darwin.close(child) }
                try scan(
                    child,
                    depth: depth + 1,
                    maximumDepth: maximumDepth,
                    cutoffUnixSeconds: cutoffUnixSeconds,
                    state: &state
                )
                if state.truncated { return }
                continue
            }
            guard isTemporaryName(name),
                  (status.st_mode & S_IFMT) == S_IFREG,
                  status.st_uid == geteuid(), status.st_nlink == 1,
                  Int64(status.st_mtimespec.tv_sec) <= cutoffUnixSeconds else { continue }
            state.eligible += 1
            let descriptor = name.withCString {
                Darwin.openat(directory, $0, O_RDWR | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
            }
            guard descriptor >= 0 else { continue }
            defer { _ = Darwin.close(descriptor) }
            guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else {
                state.locked += 1
                continue
            }
            var opened = stat()
            var current = stat()
            guard Darwin.fstat(descriptor, &opened) == 0,
                  sameIdentity(status, opened),
                  name.withCString({
                      Darwin.fstatat(directory, $0, &current, AT_SYMLINK_NOFOLLOW)
                  }) == 0,
                  sameIdentity(status, current),
                  name.withCString({ Darwin.unlinkat(directory, $0, 0) }) == 0 else {
                continue
            }
            state.deleted += 1
        }
    }

    private func openRoot() throws -> Int32 {
        let descriptor = root.path.withCString {
            Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CodeReadWorkerCoreError.notDirectory }
        var status = stat()
        guard Darwin.fstat(descriptor, &status) == 0,
              UInt64(status.st_dev) == rootIdentity.device,
              UInt64(status.st_ino) == rootIdentity.inode else {
            _ = Darwin.close(descriptor)
            throw CodeReadWorkerCoreError.invalidRequest("temporary cleanup root changed")
        }
        return descriptor
    }

    private func openExistingDirectoryChain(
        _ path: [String],
        from root: Int32
    ) throws -> Int32? {
        if path.isEmpty { return root }
        var current = root
        var ownsCurrent = false
        do {
            for component in path {
                let next = component.withCString {
                    Darwin.openat(current, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
                }
                if next < 0, errno == ENOENT {
                    if ownsCurrent { _ = Darwin.close(current) }
                    return nil
                }
                guard next >= 0 else {
                    throw CodeReadWorkerCoreError.filesystemFailure(
                        "temporary cleanup scope is unavailable"
                    )
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
        return result
    }

    private func isTemporaryName(_ name: String) -> Bool {
        if name.hasPrefix(".chatos-write-"), name.hasSuffix(".tmp") {
            return canonicalUUID(
                String(name.dropFirst(".chatos-write-".count).dropLast(".tmp".count))
            )
        }
        if name.hasPrefix(".chatos-artifact-copy-"), name.hasSuffix(".tmp") {
            return canonicalUUID(
                String(name.dropFirst(".chatos-artifact-copy-".count).dropLast(".tmp".count))
            )
        }
        return false
    }

    private func canonicalUUID(_ value: String) -> Bool {
        guard let uuid = UUID(uuidString: value) else { return false }
        return uuid.uuidString.lowercased() == value
    }

    private func sameIdentity(_ lhs: stat, _ rhs: stat) -> Bool {
        lhs.st_dev == rhs.st_dev && lhs.st_ino == rhs.st_ino
            && lhs.st_size == rhs.st_size
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

    private struct Scope {
        let path: [String]
        let maximumDepth: Int
    }

    private struct CleanupState {
        var visited = 0
        var eligible = 0
        var deleted = 0
        var locked = 0
        var truncated = false
    }
}
