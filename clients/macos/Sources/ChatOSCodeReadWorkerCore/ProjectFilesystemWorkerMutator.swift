import ChatOSCodeReadWorkerProtocol
import Darwin
import Foundation

struct ProjectFilesystemWorkerMutator {
    let root: URL
    let rootIdentity: CodeReadWorkerFileIdentity
    let deadlineUnixMilliseconds: Int64

    func execute(
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        switch operation {
        case "project_write_file_v1":
            return try write(
                path: requiredString(arguments, "path"),
                content: requiredString(arguments, "content"),
                createOnly: arguments.bool("create_only") ?? false
            )
        case "project_create_directory_v1":
            return try createDirectory(
                path: requiredString(arguments, "path"),
                createOnly: arguments.bool("create_only") ?? false
            )
        case "project_delete_entry_v1":
            guard let recursive = arguments.bool("recursive") else {
                throw CodeReadWorkerCoreError.invalidRequest("缺少布尔字段：recursive")
            }
            return try delete(path: requiredString(arguments, "path"), recursive: recursive)
        default:
            throw CodeReadWorkerCoreError.unsupportedTool(operation)
        }
    }

    private func write(path: String, content: String, createOnly: Bool) throws -> CodeReadWorkerJSONValue {
        try checkDeadline()
        let components = try normalizedComponents(path)
        let rootDescriptor = try openRoot()
        defer { _ = Darwin.close(rootDescriptor) }
        let parentDescriptor = try openDirectoryChain(
            Array(components.dropLast()),
            from: rootDescriptor
        )
        defer { if parentDescriptor != rootDescriptor { _ = Darwin.close(parentDescriptor) } }
        let leaf = components[components.count - 1]
        var existing = stat()
        let exists = leaf.withCString {
            Darwin.fstatat(parentDescriptor, $0, &existing, AT_SYMLINK_NOFOLLOW)
        } == 0
        if exists {
            guard (existing.st_mode & S_IFMT) == S_IFREG else {
                if (existing.st_mode & S_IFMT) == S_IFLNK {
                    throw CodeReadWorkerCoreError.symbolicLink
                }
                throw CodeReadWorkerCoreError.notFile
            }
            guard !createOnly else { throw CodeReadWorkerCoreError.alreadyExists }
        } else if errno != ENOENT {
            throw mappedErrno(errno)
        }

        let data = Data(content.utf8)
        if createOnly {
            let descriptor = try openNewFile(named: leaf, in: parentDescriptor)
            do {
                try writeAll(data, to: descriptor)
                guard Darwin.fsync(descriptor) == 0 else { throw mappedErrno(errno) }
            } catch {
                _ = Darwin.close(descriptor)
                leaf.withCString { _ = Darwin.unlinkat(parentDescriptor, $0, 0) }
                throw error
            }
            _ = Darwin.close(descriptor)
        } else {
            let temporary = ".chatos-write-\(UUID().uuidString.lowercased()).tmp"
            let descriptor = try openNewFile(named: temporary, in: parentDescriptor)
            do {
                if exists {
                    guard Darwin.fchmod(descriptor, existing.st_mode & 0o7777) == 0 else {
                        throw mappedErrno(errno)
                    }
                }
                try writeAll(data, to: descriptor)
                guard Darwin.fsync(descriptor) == 0 else { throw mappedErrno(errno) }
                let renameResult = temporary.withCString { source in
                    leaf.withCString { target in
                        Darwin.renameat(parentDescriptor, source, parentDescriptor, target)
                    }
                }
                guard renameResult == 0 else { throw mappedErrno(errno) }
                _ = Darwin.close(descriptor)
            } catch {
                _ = Darwin.close(descriptor)
                temporary.withCString { _ = Darwin.unlinkat(parentDescriptor, $0, 0) }
                throw error
            }
        }
        return .object([
            "path": .string(components.joined(separator: "/")),
            "size": .number(Double(data.count)),
            "created": .bool(!exists),
        ])
    }

    private func createDirectory(
        path: String,
        createOnly: Bool
    ) throws -> CodeReadWorkerJSONValue {
        let components = try normalizedComponents(path)
        let rootDescriptor = try openRoot()
        defer { _ = Darwin.close(rootDescriptor) }
        var current = rootDescriptor
        var ownsCurrent = false
        var created = false
        defer { if ownsCurrent { _ = Darwin.close(current) } }
        for (index, component) in components.enumerated() {
            try checkDeadline()
            let result = component.withCString { Darwin.mkdirat(current, $0, 0o755) }
            if result == 0 {
                created = true
            } else if errno != EEXIST {
                throw mappedErrno(errno)
            } else if createOnly, index == components.count - 1 {
                throw CodeReadWorkerCoreError.alreadyExists
            }
            let next = component.withCString {
                Darwin.openat(current, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            }
            guard next >= 0 else {
                throw mappedDirectoryOpenErrno(errno, name: component, in: current)
            }
            if ownsCurrent { _ = Darwin.close(current) }
            current = next
            ownsCurrent = true
        }
        return .object([
            "path": .string(components.joined(separator: "/")),
            "created": .bool(created),
        ])
    }

    private func delete(path: String, recursive: Bool) throws -> CodeReadWorkerJSONValue {
        try checkDeadline()
        let components = try normalizedComponents(path)
        let rootDescriptor = try openRoot()
        defer { _ = Darwin.close(rootDescriptor) }
        let parentDescriptor = try openDirectoryChain(
            Array(components.dropLast()),
            from: rootDescriptor
        )
        defer { if parentDescriptor != rootDescriptor { _ = Darwin.close(parentDescriptor) } }
        let leaf = components[components.count - 1]
        var status = stat()
        let statResult = leaf.withCString {
            Darwin.fstatat(parentDescriptor, $0, &status, AT_SYMLINK_NOFOLLOW)
        }
        guard statResult == 0 else { throw mappedErrno(errno) }
        let isDirectory = (status.st_mode & S_IFMT) == S_IFDIR
        if isDirectory && recursive {
            let descriptor = leaf.withCString {
                Darwin.openat(parentDescriptor, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            }
            guard descriptor >= 0 else { throw mappedErrno(errno) }
            defer { _ = Darwin.close(descriptor) }
            try validateIdentity(descriptor, expected: status)
            try deleteDirectoryContents(descriptor)
            try validateIdentity(leaf, in: parentDescriptor, expected: status)
        }
        let flags = isDirectory ? AT_REMOVEDIR : 0
        let result = leaf.withCString { Darwin.unlinkat(parentDescriptor, $0, flags) }
        guard result == 0 else {
            if isDirectory && !recursive && (errno == ENOTEMPTY || errno == EEXIST) {
                throw CodeReadWorkerCoreError.directoryNotEmpty
            }
            throw mappedErrno(errno)
        }
        return .object([
            "path": .string(components.joined(separator: "/")),
            "is_dir": .bool(isDirectory),
            "recursive": .bool(recursive),
            "deleted": .bool(true),
        ])
    }

    private func deleteDirectoryContents(_ descriptor: Int32) throws {
        guard let directory = Darwin.fdopendir(Darwin.dup(descriptor)) else {
            throw mappedErrno(errno)
        }
        defer { Darwin.closedir(directory) }
        while let entry = Darwin.readdir(directory) {
            try checkDeadline()
            let name = withUnsafePointer(to: &entry.pointee.d_name) {
                $0.withMemoryRebound(to: CChar.self, capacity: Int(MAXNAMLEN) + 1) {
                    String(cString: $0)
                }
            }
            if name == "." || name == ".." { continue }
            var status = stat()
            let statResult = name.withCString {
                Darwin.fstatat(descriptor, $0, &status, AT_SYMLINK_NOFOLLOW)
            }
            if statResult != 0 {
                if errno == ENOENT { continue }
                throw mappedErrno(errno)
            }
            if (status.st_mode & S_IFMT) == S_IFDIR {
                let child = name.withCString {
                    Darwin.openat(descriptor, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
                }
                guard child >= 0 else { throw mappedErrno(errno) }
                do {
                    try validateIdentity(child, expected: status)
                    try deleteDirectoryContents(child)
                    try validateIdentity(name, in: descriptor, expected: status)
                    _ = Darwin.close(child)
                } catch {
                    _ = Darwin.close(child)
                    throw error
                }
                let result = name.withCString { Darwin.unlinkat(descriptor, $0, AT_REMOVEDIR) }
                guard result == 0 else { throw mappedErrno(errno) }
            } else {
                let result = name.withCString { Darwin.unlinkat(descriptor, $0, 0) }
                guard result == 0 else { throw mappedErrno(errno) }
            }
        }
    }

    private func openRoot() throws -> Int32 {
        let descriptor = root.path.withCString {
            Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw mappedErrno(errno) }
        var status = stat()
        guard Darwin.fstat(descriptor, &status) == 0,
              UInt64(status.st_dev) == rootIdentity.device,
              UInt64(status.st_ino) == rootIdentity.inode else {
            _ = Darwin.close(descriptor)
            throw CodeReadWorkerCoreError.invalidRequest("项目根目录身份已经变化")
        }
        return descriptor
    }

    private func openDirectoryChain(_ components: [String], from rootDescriptor: Int32) throws -> Int32 {
        var current = rootDescriptor
        var ownsCurrent = false
        do {
            for component in components {
                try checkDeadline()
                let next = component.withCString {
                    Darwin.openat(current, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
                }
                guard next >= 0 else {
                    throw mappedDirectoryOpenErrno(errno, name: component, in: current)
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

    private func openNewFile(named name: String, in parentDescriptor: Int32) throws -> Int32 {
        let descriptor = name.withCString {
            Darwin.openat(
                parentDescriptor,
                $0,
                O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
                0o644
            )
        }
        guard descriptor >= 0 else { throw mappedErrno(errno) }
        return descriptor
    }

    private func writeAll(_ data: Data, to descriptor: Int32) throws {
        try data.withUnsafeBytes { bytes in
            guard let base = bytes.baseAddress else { return }
            var offset = 0
            while offset < bytes.count {
                try checkDeadline()
                let count = Darwin.write(descriptor, base.advanced(by: offset), bytes.count - offset)
                if count < 0 {
                    if errno == EINTR { continue }
                    throw mappedErrno(errno)
                }
                offset += count
            }
        }
    }

    private func normalizedComponents(_ path: String) throws -> [String] {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.hasPrefix("/"), !trimmed.contains("\0") else {
            throw CodeReadWorkerCoreError.pathOutsideProject
        }
        var components: [String] = []
        for raw in trimmed.split(separator: "/", omittingEmptySubsequences: true) {
            let component = String(raw)
            if component == "." { continue }
            guard component != ".." else { throw CodeReadWorkerCoreError.pathOutsideProject }
            components.append(component)
        }
        guard !components.isEmpty else { throw CodeReadWorkerCoreError.rootMutation }
        return components
    }

    private func requiredString(
        _ arguments: [String: CodeReadWorkerJSONValue],
        _ key: String
    ) throws -> String {
        guard let value = arguments.string(key) else {
            throw CodeReadWorkerCoreError.invalidRequest("缺少字符串字段：\(key)")
        }
        return value
    }

    private func checkDeadline() throws {
        guard Int64(Date().timeIntervalSince1970 * 1_000) <= deadlineUnixMilliseconds else {
            throw CodeReadWorkerCoreError.deadlineExceeded
        }
    }

    private func mappedErrno(_ value: Int32) -> CodeReadWorkerCoreError {
        switch value {
        case ENOENT: .notFound
        case ENOTDIR: .notDirectory
        case EISDIR: .notFile
        case ELOOP: .symbolicLink
        case EEXIST: .alreadyExists
        case ENOTEMPTY: .directoryNotEmpty
        default:
            .filesystemFailure(String(cString: strerror(value)))
        }
    }

    private func mappedDirectoryOpenErrno(
        _ value: Int32,
        name: String,
        in parentDescriptor: Int32
    ) -> CodeReadWorkerCoreError {
        guard value == ELOOP || value == ENOTDIR else { return mappedErrno(value) }
        var status = stat()
        let result = name.withCString {
            Darwin.fstatat(parentDescriptor, $0, &status, AT_SYMLINK_NOFOLLOW)
        }
        if result == 0, (status.st_mode & S_IFMT) == S_IFLNK {
            return .symbolicLink
        }
        return mappedErrno(value)
    }

    private func validateIdentity(_ descriptor: Int32, expected: stat) throws {
        var current = stat()
        guard Darwin.fstat(descriptor, &current) == 0,
              current.st_dev == expected.st_dev,
              current.st_ino == expected.st_ino else {
            throw CodeReadWorkerCoreError.filesystemFailure("目标在操作期间发生变化")
        }
    }

    private func validateIdentity(
        _ name: String,
        in parentDescriptor: Int32,
        expected: stat
    ) throws {
        var current = stat()
        let result = name.withCString {
            Darwin.fstatat(parentDescriptor, $0, &current, AT_SYMLINK_NOFOLLOW)
        }
        guard result == 0,
              current.st_dev == expected.st_dev,
              current.st_ino == expected.st_ino else {
            throw CodeReadWorkerCoreError.filesystemFailure("目标在操作期间发生变化")
        }
    }
}

private extension Dictionary where Key == String, Value == CodeReadWorkerJSONValue {
    func string(_ key: String) -> String? {
        guard case let .string(value)? = self[key] else { return nil }
        return value
    }

    func bool(_ key: String) -> Bool? {
        guard case let .bool(value)? = self[key] else { return nil }
        return value
    }
}
