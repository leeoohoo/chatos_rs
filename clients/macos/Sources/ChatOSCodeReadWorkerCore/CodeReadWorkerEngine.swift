import ChatOSCodeReadWorkerProtocol
import CryptoKit
import Darwin
import Foundation

public enum CodeReadWorkerCoreError: LocalizedError, Sendable {
    case invalidRequest(String)
    case pathOutsideProject
    case notFound
    case notDirectory
    case notFile
    case fileTooLarge(Int)
    case binaryFile
    case deadlineExceeded
    case searchFailed(String)
    case unsupportedTool(String)
    case rootMutation
    case symbolicLink
    case alreadyExists
    case directoryNotEmpty
    case filesystemFailure(String)
    case attachmentInvalid
    case attachmentReferenceInvalid
    case attachmentIntegrityMismatch
    case attachmentWriteFailed
    case attachmentWorkspaceUnavailable
    case attachmentWorkspaceConflict
    case artifactInvalid
    case artifactWriteFailed

    public var errorDescription: String? {
        switch self {
        case let .invalidRequest(message): message
        case .pathOutsideProject: "路径超出当前项目范围"
        case .notFound: "目标不存在"
        case .notDirectory: "目标不是目录"
        case .notFile: "目标不是普通文件"
        case let .fileTooLarge(size): "文件超过读取限制：\(size) bytes"
        case .binaryFile: "二进制文件不支持按文本读取"
        case .deadlineExceeded: "项目代码读取超时，独立处理进程已终止"
        case let .searchFailed(message): message
        case let .unsupportedTool(name): "暂不支持 MCP 工具：\(name)"
        case .rootMutation: "不能修改授权工作区根目录"
        case .symbolicLink: "操作不能穿过符号链接"
        case .alreadyExists: "目标已经存在"
        case .directoryNotEmpty: "目录不为空，请使用递归删除"
        case let .filesystemFailure(message): message
        case .attachmentInvalid: "The local attachment is unavailable or exceeds its safety limit."
        case .attachmentReferenceInvalid: "The local attachment reference is invalid."
        case .attachmentIntegrityMismatch: "The local attachment failed its integrity check."
        case .attachmentWriteFailed: "The local attachment could not be stored safely."
        case .attachmentWorkspaceUnavailable:
            "The authorized attachment could not be imported into the task workspace."
        case .attachmentWorkspaceConflict:
            "The workspace attachment copy has changed or is unsafe; it was not overwritten."
        case .artifactInvalid: "The artifact file failed its integrity check."
        case .artifactWriteFailed: "The artifact file could not be stored safely."
        }
    }

    public var payload: CodeReadWorkerErrorPayload {
        let code: String
        switch self {
        case .invalidRequest: code = "invalid_request"
        case .pathOutsideProject: code = "path_outside_project"
        case .notFound: code = "not_found"
        case .notDirectory: code = "not_directory"
        case .notFile: code = "not_file"
        case .fileTooLarge: code = "file_too_large"
        case .binaryFile: code = "binary_file"
        case .deadlineExceeded: code = "deadline_exceeded"
        case .searchFailed: code = "search_failed"
        case .unsupportedTool: code = "unsupported_tool"
        case .rootMutation: code = "root_mutation"
        case .symbolicLink: code = "symbolic_link"
        case .alreadyExists: code = "already_exists"
        case .directoryNotEmpty: code = "directory_not_empty"
        case .filesystemFailure: code = "filesystem_failure"
        case .attachmentInvalid: code = "attachment_invalid"
        case .attachmentReferenceInvalid: code = "attachment_reference_invalid"
        case .attachmentIntegrityMismatch: code = "attachment_integrity_mismatch"
        case .attachmentWriteFailed: code = "attachment_write_failed"
        case .attachmentWorkspaceUnavailable: code = "attachment_workspace_unavailable"
        case .attachmentWorkspaceConflict: code = "attachment_workspace_conflict"
        case .artifactInvalid: code = "artifact_invalid"
        case .artifactWriteFailed: code = "artifact_write_failed"
        }
        return .init(code: code, message: errorDescription ?? code)
    }
}

public enum ChatOSCodeReadWorkerEngine {
    fileprivate static let maximumReadBytes = 2 * 1_024 * 1_024
    fileprivate static let maximumProjectDirectoryEntries = 5_000
    fileprivate static let maximumSearchVisits = 20_000
    fileprivate static let ignoredGlobs = [
        "!.git/**", "!.build/**", "!.cache/**", "!.next/**", "!.idea/**",
        "!.vscode/**", "!node_modules/**", "!DerivedData/**", "!Pods/**",
        "!target/**", "!dist/**", "!build/**", "!vendor/**",
    ]

    public static func execute(_ request: CodeReadWorkerRequest) throws -> CodeReadWorkerJSONValue {
        guard request.type == "request",
              request.protocolVersion == ChatOSCodeReadWorkerProtocol.maximumVersion,
              ChatOSCodeReadWorkerProtocol.operations.contains(request.operation),
              request.binaryPayloadByteCount == 0,
              request.deadlineUnixMilliseconds > nowMilliseconds() else {
            throw CodeReadWorkerCoreError.invalidRequest("代码读取请求无效或已经过期")
        }
        let root = try validatedRoot(
            path: request.projectRootPath,
            identity: request.projectRootIdentity
        )
        let context = Context(
            root: root,
            deadlineUnixMilliseconds: request.deadlineUnixMilliseconds,
            requestCWD: request.requestCWD,
            defaultToolRoot: request.defaultToolRoot,
            ripgrepURL: try validatedRipgrep(request)
        )
        switch request.operation {
        case "read_file_raw":
            return try context.readFile(request.arguments)
        case "read_file_range":
            return try context.readRange(request.arguments)
        case "list_dir":
            return try context.listDirectory(request.arguments)
        case "search_text":
            return try context.searchText(request.arguments)
        case "read_file":
            if request.arguments.number("start_line") != nil
                || request.arguments.number("end_line") != nil {
                guard request.arguments.number("start_line") != nil,
                      request.arguments.number("end_line") != nil else {
                    throw CodeReadWorkerCoreError.invalidRequest(
                        "start_line 与 end_line 必须同时提供"
                    )
                }
                return try context.readRange(request.arguments)
            }
            return try context.readFile(request.arguments)
        case "search_files":
            var mapped = request.arguments
            if mapped["pattern"] == nil, let query = request.arguments.string("query") {
                mapped["pattern"] = .string(query)
            }
            return try context.searchText(mapped)
        case "open_file_in_pet":
            return try context.resolvePetFile(request.arguments)
        case "verify_file_identity_v1":
            return try context.verifyFileIdentity(request.arguments)
        case "artifact_stage_v1", "artifact_restore_v1":
            return try ArtifactWorkerOperations(
                root: root,
                rootIdentity: request.projectRootIdentity,
                deadlineUnixMilliseconds: request.deadlineUnixMilliseconds
            ).execute(operation: request.operation, arguments: request.arguments)
        case "cleanup_stale_temporary_files_v1":
            return try TemporaryFileCleanupWorkerOperations(
                root: root,
                rootIdentity: request.projectRootIdentity,
                deadlineUnixMilliseconds: request.deadlineUnixMilliseconds
            ).execute(arguments: request.arguments)
        case "project_list_v1":
            return try context.projectList(request.arguments)
        case "project_search_entries_v1":
            return try context.projectSearchEntries(request.arguments)
        case "project_run_analyze_v1":
            return try ProjectRunWorkerAnalyzer(
                root: root,
                deadlineUnixMilliseconds: request.deadlineUnixMilliseconds
            ).analyze(request.arguments)
        case "project_write_file_v1", "project_create_directory_v1", "project_delete_entry_v1":
            return try ProjectFilesystemWorkerMutator(
                root: root,
                rootIdentity: request.projectRootIdentity,
                deadlineUnixMilliseconds: request.deadlineUnixMilliseconds
            ).execute(operation: request.operation, arguments: request.arguments)
        case "attachment_resolve_v1", "attachment_verify_file_v1", "attachment_materialize_v1":
            return try AttachmentWorkerOperations(
                root: root,
                rootIdentity: request.projectRootIdentity,
                deadlineUnixMilliseconds: request.deadlineUnixMilliseconds
            ).execute(operation: request.operation, arguments: request.arguments)
        default:
            throw CodeReadWorkerCoreError.unsupportedTool(request.operation)
        }
    }

    public static func executeStreaming(
        _ request: CodeReadWorkerRequest,
        readBinaryChunk: (Int) throws -> Data
    ) throws -> CodeReadWorkerJSONValue {
        guard request.type == "request",
              request.protocolVersion == ChatOSCodeReadWorkerProtocol.maximumVersion,
              request.operation == "attachment_authorize_v1",
              request.binaryPayloadByteCount > 0,
              request.binaryPayloadByteCount
                <= UInt64(ChatOSCodeReadWorkerProtocol.maximumBinaryPayloadBytes),
              request.deadlineUnixMilliseconds > nowMilliseconds() else {
            throw CodeReadWorkerCoreError.invalidRequest("附件授权请求无效或已经过期")
        }
        let root = try validatedRoot(
            path: request.projectRootPath,
            identity: request.projectRootIdentity
        )
        return try AttachmentWorkerOperations(
            root: root,
            rootIdentity: request.projectRootIdentity,
            deadlineUnixMilliseconds: request.deadlineUnixMilliseconds
        ).authorize(
            arguments: request.arguments,
            binaryPayloadByteCount: request.binaryPayloadByteCount,
            readBinaryChunk: readBinaryChunk
        )
    }

    private static func validatedRoot(
        path: String,
        identity: CodeReadWorkerFileIdentity
    ) throws -> URL {
        let raw = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL
        let resolved = raw.resolvingSymlinksInPath().standardizedFileURL
        guard raw.path == resolved.path else { throw CodeReadWorkerCoreError.pathOutsideProject }
        var status = stat()
        let descriptor = resolved.path.withCString {
            Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CodeReadWorkerCoreError.notDirectory }
        defer { _ = Darwin.close(descriptor) }
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFDIR,
              UInt64(status.st_dev) == identity.device,
              UInt64(status.st_ino) == identity.inode else {
            throw CodeReadWorkerCoreError.invalidRequest("项目根目录身份已经变化")
        }
        return resolved
    }

    private static func validatedRipgrep(_ request: CodeReadWorkerRequest) throws -> URL? {
        guard let path = request.ripgrepPath,
              let identity = request.ripgrepIdentity else { return nil }
        let url = URL(fileURLWithPath: path).standardizedFileURL
        var status = stat()
        let descriptor = url.path.withCString {
            Darwin.open($0, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { return nil }
        defer { _ = Darwin.close(descriptor) }
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              (status.st_mode & S_IXUSR) != 0,
              UInt64(status.st_dev) == identity.device,
              UInt64(status.st_ino) == identity.inode else { return nil }
        return url
    }

    private static func nowMilliseconds() -> Int64 {
        Int64(Date().timeIntervalSince1970 * 1_000)
    }
}

private struct Context {
    let root: URL
    let deadlineUnixMilliseconds: Int64
    let requestCWD: String?
    let defaultToolRoot: String?
    let ripgrepURL: URL?

    func readFile(
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        let path = try normalizedToolPath(requiredString(arguments, "path"))
        let file = try readTextFile(path: path)
        let lines = normalizedLines(file.content)
        var payload: [String: CodeReadWorkerJSONValue] = [
            "path": .string(path),
            "size_bytes": .number(Double(file.byteCount)),
            "sha256": .string(file.sha256),
            "line_count": .number(Double(lines.count)),
            "ends_with_newline": .bool(file.content.hasSuffix("\n")),
            "content": .string(file.content),
        ]
        if arguments.bool("with_line_numbers") ?? true {
            payload["numbered_lines"] = .array(lines.enumerated().map { index, line in
                .object(["line": .number(Double(index + 1)), "text": .string(line)])
            })
        }
        return .object(payload)
    }

    func readRange(
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        let path = try normalizedToolPath(requiredString(arguments, "path"))
        let start = max(1, Int(try requiredNumber(arguments, "start_line")))
        let requestedEnd = max(1, Int(try requiredNumber(arguments, "end_line")))
        let file = try readTextFile(path: path)
        let lines = normalizedLines(file.content)
        let end = min(requestedEnd, max(1, lines.count))
        let selected: [String]
        if start <= end {
            selected = (start...end).map { lineNumber in
                let text = lines[lineNumber - 1]
                return arguments.bool("with_line_numbers") == true
                    ? "\(lineNumber): \(text)"
                    : text
            }
        } else {
            selected = []
        }
        return .object([
            "path": .string(path),
            "size_bytes": .number(Double(file.byteCount)),
            "sha256": .string(file.sha256),
            "start_line": .number(Double(start)),
            "end_line": .number(Double(end)),
            "total_lines": .number(Double(lines.count)),
            "content": .string(selected.joined(separator: "\n")),
        ])
    }

    func resolvePetFile(
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        try checkDeadline()
        let path = try normalizedToolPath(requiredString(arguments, "path"))
        let url = try resolveExisting(path)
        let descriptor = url.path.withCString {
            Darwin.open($0, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CodeReadWorkerCoreError.notFile }
        defer { _ = Darwin.close(descriptor) }
        var status = stat()
        guard Darwin.fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG else {
            throw CodeReadWorkerCoreError.notFile
        }
        let mode = arguments.string("mode") ?? "preview"
        guard mode == "preview" || mode == "edit" else {
            throw CodeReadWorkerCoreError.invalidRequest("mode 必须是 preview 或 edit")
        }
        let line: CodeReadWorkerJSONValue
        if let rawLine = arguments.number("line") {
            guard rawLine.isFinite, rawLine >= 1, rawLine.rounded() == rawLine,
                  rawLine <= 9_007_199_254_740_991 else {
                throw CodeReadWorkerCoreError.invalidRequest("line 必须是正整数")
            }
            line = .number(rawLine)
        } else {
            line = .null
        }
        return .object([
            "relative_path": .string(relativePath(url)),
            "line": line,
            "mode": .string(mode),
        ])
    }

    func verifyFileIdentity(
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        try checkDeadline()
        let name = try requiredString(arguments, "name")
        let expectedBytesValue = try requiredNumber(arguments, "byte_size")
        let expectedDigest = try requiredString(arguments, "sha256").lowercased()
        guard !name.contains("/"), !name.contains("\\"), name != ".", name != "..",
              name.utf8.count <= 255,
              expectedBytesValue.isFinite,
              expectedBytesValue.rounded() == expectedBytesValue,
              expectedBytesValue > 0,
              expectedBytesValue <= 20 * 1_024 * 1_024,
              expectedDigest.utf8.count == 64,
              expectedDigest.utf8.allSatisfy({
                  (48...57).contains($0) || (97...102).contains($0)
              }) else {
            throw CodeReadWorkerCoreError.invalidRequest("文件身份校验参数无效")
        }
        let expectedBytes = Int64(expectedBytesValue)
        let rootDescriptor = root.path.withCString {
            Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard rootDescriptor >= 0 else { throw CodeReadWorkerCoreError.notDirectory }
        defer { _ = Darwin.close(rootDescriptor) }
        let descriptor = name.withCString {
            Darwin.openat(rootDescriptor, $0, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CodeReadWorkerCoreError.notFile }
        defer { _ = Darwin.close(descriptor) }

        var before = stat()
        guard Darwin.fstat(descriptor, &before) == 0,
              (before.st_mode & S_IFMT) == S_IFREG,
              before.st_uid == geteuid(),
              before.st_size == expectedBytes,
              Darwin.lseek(descriptor, 0, SEEK_SET) == 0 else {
            throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
        }
        var hasher = SHA256()
        var total: Int64 = 0
        var buffer = [UInt8](repeating: 0, count: 64 * 1_024)
        while total < expectedBytes {
            try checkDeadline()
            let requested = min(buffer.count, Int(expectedBytes - total))
            let count = buffer.withUnsafeMutableBytes { bytes in
                Darwin.read(descriptor, bytes.baseAddress, requested)
            }
            if count < 0, errno == EINTR { continue }
            guard count > 0 else {
                throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
            }
            buffer.withUnsafeBytes { bytes in
                hasher.update(bufferPointer: UnsafeRawBufferPointer(
                    rebasing: bytes.prefix(count)
                ))
            }
            total += Int64(count)
        }
        var trailingByte: UInt8 = 0
        while true {
            let trailingCount = Darwin.read(descriptor, &trailingByte, 1)
            if trailingCount == 0 { break }
            if trailingCount < 0, errno == EINTR { continue }
            throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
        }
        var after = stat()
        let digest = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        guard Darwin.fstat(descriptor, &after) == 0,
              before.st_dev == after.st_dev,
              before.st_ino == after.st_ino,
              before.st_size == after.st_size,
              before.st_mtimespec.tv_sec == after.st_mtimespec.tv_sec,
              before.st_mtimespec.tv_nsec == after.st_mtimespec.tv_nsec,
              before.st_ctimespec.tv_sec == after.st_ctimespec.tv_sec,
              before.st_ctimespec.tv_nsec == after.st_ctimespec.tv_nsec,
              digest == expectedDigest else {
            throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
        }
        return .object([
            "device": .string(String(after.st_dev)),
            "inode": .string(String(after.st_ino)),
            "byte_count": .number(Double(after.st_size)),
            "modified_seconds": .string(String(after.st_mtimespec.tv_sec)),
            "modified_nanoseconds": .string(String(after.st_mtimespec.tv_nsec)),
        ])
    }

    func listDirectory(
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        try checkDeadline()
        let path = try normalizedToolPath(arguments.string("path") ?? ".")
        let maximum = min(max(Int(arguments.number("max_entries") ?? 200), 1), 1_000)
        let directory = try resolveExisting(path)
        let values = try directory.resourceValues(forKeys: [.isDirectoryKey])
        guard values.isDirectory == true else { throw CodeReadWorkerCoreError.notDirectory }
        let keys: Set<URLResourceKey> = [
            .isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey,
            .fileSizeKey, .contentModificationDateKey,
        ]
        guard let enumerator = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: Array(keys),
            options: [.skipsSubdirectoryDescendants],
            errorHandler: { _, _ in false }
        ) else { throw CodeReadWorkerCoreError.notDirectory }
        var urls: [URL] = []
        var truncated = false
        while let url = enumerator.nextObject() as? URL {
            try checkDeadline()
            if urls.count >= maximum {
                truncated = true
                break
            }
            urls.append(url)
        }
        let entries = try urls.compactMap { url -> Entry? in
            let values = try url.resourceValues(forKeys: keys)
            guard values.isSymbolicLink != true else { return nil }
            return Entry(
                name: url.lastPathComponent,
                path: relativePath(url),
                isDirectory: values.isDirectory == true,
                size: Int64(values.fileSize ?? 0),
                modifiedAt: values.contentModificationDate.map {
                    Int64($0.timeIntervalSince1970 * 1_000)
                }
            )
        }.sorted {
            if $0.isDirectory != $1.isDirectory { return $0.isDirectory && !$1.isDirectory }
            return $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }
        return .object([
            "entries": .array(entries.map(\.jsonValue)),
            "truncated": .bool(truncated),
        ])
    }

    func searchText(
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        let pattern = try requiredString(arguments, "pattern")
        let path = try normalizedToolPath(arguments.string("path") ?? ".")
        let maximum = min(max(Int(arguments.number("max_results") ?? 200), 1), 500)
        let caseInsensitive = arguments.bool("case_insensitive") ?? false
        let partialOnTimeout = arguments.bool("partial_on_timeout") ?? false
        let maximumLineCharacters = min(
            max(Int(arguments.number("max_line_characters") ?? 400), 100),
            2_000
        )
        let searchDeadline = operationDeadline(arguments)
        let start = try resolveExisting(path)
        let search = if let ripgrepURL {
            try RipgrepSearch.search(
                executable: ripgrepURL,
                pattern: pattern,
                start: start,
                root: root,
                maximumResults: maximum,
                caseInsensitive: caseInsensitive,
                maximumLineCharacters: maximumLineCharacters,
                partialOnTimeout: partialOnTimeout,
                deadlineUnixMilliseconds: searchDeadline
            )
        } else {
            try fallbackSearch(
                pattern: pattern,
                start: start,
                maximum: maximum,
                caseInsensitive: caseInsensitive,
                maximumLineCharacters: maximumLineCharacters,
                partialOnTimeout: partialOnTimeout,
                searchDeadline: searchDeadline
            )
        }
        return .object([
            "count": .number(Double(search.results.count)),
            "results": .array(search.results),
            "truncated": .bool(search.truncated),
        ])
    }

    private func fallbackSearch(
        pattern: String,
        start: URL,
        maximum: Int,
        caseInsensitive: Bool,
        maximumLineCharacters: Int,
        partialOnTimeout: Bool,
        searchDeadline: Int64
    ) throws -> (results: [CodeReadWorkerJSONValue], truncated: Bool) {
        var stack = [start]
        var visits = 0
        var results: [CodeReadWorkerJSONValue] = []
        var truncated = false
        while let url = stack.popLast() {
            if deadlinePassed(searchDeadline), partialOnTimeout {
                truncated = true
                break
            }
            try checkDeadline(searchDeadline)
            guard visits < ChatOSCodeReadWorkerEngine.maximumSearchVisits else {
                truncated = true
                break
            }
            visits += 1
            let values = try? url.resourceValues(forKeys: [
                .isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
            ])
            guard values?.isSymbolicLink != true else { continue }
            if values?.isDirectory == true {
                if url != start, shouldSkip(url) { continue }
                if let children = try? FileManager.default.contentsOfDirectory(
                    at: url,
                    includingPropertiesForKeys: nil
                ) {
                    stack.append(contentsOf: children)
                }
                continue
            }
            guard values?.isRegularFile == true,
                  (values?.fileSize ?? Int.max) <= ChatOSCodeReadWorkerEngine.maximumReadBytes,
                  let file = try? readTextFile(url: url) else { continue }
            let options: String.CompareOptions = caseInsensitive
                ? [.caseInsensitive, .diacriticInsensitive]
                : []
            for (index, line) in normalizedLines(file.content).enumerated() {
                guard let range = line.range(of: pattern, options: options) else { continue }
                results.append(.object([
                    "path": .string(relativePath(url)),
                    "line": .number(Double(index + 1)),
                    "column": .number(Double(
                        line.distance(from: line.startIndex, to: range.lowerBound) + 1
                    )),
                    "text": .string(String(line.prefix(maximumLineCharacters))),
                ]))
                if results.count >= maximum {
                    truncated = true
                    return (results, truncated)
                }
            }
        }
        return (results, truncated)
    }

    func projectList(
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        try checkDeadline()
        let path = try normalizedToolPath(arguments.string("path") ?? ".")
        let maximum = min(
            max(Int(arguments.number("max_entries") ?? 5_000), 1),
            ChatOSCodeReadWorkerEngine.maximumProjectDirectoryEntries
        )
        let directory = try resolveExisting(path)
        let directoryValues = try directory.resourceValues(forKeys: [.isDirectoryKey])
        guard directoryValues.isDirectory == true else {
            throw CodeReadWorkerCoreError.notDirectory
        }
        let keys: Set<URLResourceKey> = [
            .isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey,
            .fileSizeKey, .contentModificationDateKey,
        ]
        let enumerated = try directChildren(
            of: directory,
            keys: keys,
            maximumEntries: maximum,
            deadline: deadlineUnixMilliseconds
        )
        let entries = try enumerated.urls.compactMap { url -> WorkspaceEntry? in
            try checkDeadline()
            let values = try url.resourceValues(forKeys: keys)
            guard values.isSymbolicLink != true else { return nil }
            return WorkspaceEntry(
                name: url.lastPathComponent,
                path: relativePath(url),
                isDirectory: values.isDirectory == true,
                size: Int64(values.fileSize ?? 0),
                modifiedAt: values.contentModificationDate.map {
                    Int64($0.timeIntervalSince1970 * 1_000)
                }
            )
        }.sorted {
            if $0.isDirectory != $1.isDirectory { return $0.isDirectory && !$1.isDirectory }
            return $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }
        let relative = relativePath(directory)
        let parent: CodeReadWorkerJSONValue = directory.path == root.path
            ? .null
            : .string(relativePath(directory.deletingLastPathComponent()))
        return .object([
            "path": .string(relative),
            "parent": parent,
            "entries": .array(entries.map(\.jsonValue)),
            "truncated": .bool(enumerated.truncated),
        ])
    }

    func projectSearchEntries(
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        let path = try normalizedToolPath(arguments.string("path") ?? ".")
        let needle = try requiredString(arguments, "query")
        let maximum = min(max(Int(arguments.number("max_results") ?? 500), 1), 500)
        let searchDeadline = operationDeadline(
            arguments,
            defaultDurationMilliseconds: 3_000
        )
        let start = try resolveExisting(path)
        guard try start.resourceValues(forKeys: [.isDirectoryKey]).isDirectory == true else {
            throw CodeReadWorkerCoreError.notDirectory
        }
        let keys: Set<URLResourceKey> = [
            .isDirectoryKey, .isSymbolicLinkKey,
            .fileSizeKey, .contentModificationDateKey,
        ]
        var stack = [start]
        var matches: [WorkspaceEntry] = []
        var visitedDirectories = 0
        var visitedEntries = 0
        var truncated = false

        while let directory = stack.popLast() {
            guard !deadlinePassed(searchDeadline),
                  visitedDirectories < ChatOSCodeReadWorkerEngine.maximumSearchVisits else {
                truncated = true
                break
            }
            visitedDirectories += 1
            let remainingVisits = ChatOSCodeReadWorkerEngine.maximumSearchVisits - visitedEntries
            guard remainingVisits > 0 else {
                truncated = true
                break
            }
            let enumerated = try? directChildren(
                of: directory,
                keys: keys,
                maximumEntries: remainingVisits,
                deadline: searchDeadline
            )
            let urls = enumerated?.urls ?? []
            visitedEntries += urls.count
            truncated = truncated || enumerated?.truncated == true
                || deadlinePassed(searchDeadline)
            for url in urls {
                guard !deadlinePassed(searchDeadline), matches.count < maximum else {
                    truncated = true
                    break
                }
                let values = try? url.resourceValues(forKeys: keys)
                guard values?.isSymbolicLink != true else { continue }
                let isDirectory = values?.isDirectory == true
                if isDirectory, !shouldSkip(url) { stack.append(url) }
                let relative = relativePath(url)
                guard url.lastPathComponent.localizedCaseInsensitiveContains(needle)
                        || relative.localizedCaseInsensitiveContains(needle) else { continue }
                matches.append(.init(
                    name: url.lastPathComponent,
                    path: relative,
                    isDirectory: isDirectory,
                    size: Int64(values?.fileSize ?? 0),
                    modifiedAt: values?.contentModificationDate.map {
                        Int64($0.timeIntervalSince1970 * 1_000)
                    }
                ))
            }
            if truncated { break }
        }
        matches.sort {
            $0.path.localizedCaseInsensitiveCompare($1.path) == .orderedAscending
        }
        return .object([
            "matches": .array(matches.map(\.jsonValue)),
            "visited_dirs": .number(Double(visitedDirectories)),
            "visited_entries": .number(Double(visitedEntries)),
            "truncated": .bool(truncated),
        ])
    }

    private func directChildren(
        of directory: URL,
        keys: Set<URLResourceKey>,
        maximumEntries: Int,
        deadline: Int64
    ) throws -> (urls: [URL], truncated: Bool) {
        guard let enumerator = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: Array(keys),
            options: [.skipsSubdirectoryDescendants],
            errorHandler: { _, _ in false }
        ) else { throw CodeReadWorkerCoreError.notDirectory }
        var urls: [URL] = []
        urls.reserveCapacity(min(maximumEntries, 1_024))
        while let url = enumerator.nextObject() as? URL {
            try checkDeadline(deadline)
            if urls.count >= maximumEntries { return (urls, true) }
            urls.append(url)
        }
        return (urls, false)
    }

    private func normalizedToolPath(_ rawPath: String) throws -> String {
        let trimmed = rawPath.trimmingCharacters(in: .whitespacesAndNewlines)
        var path = try projectRelativePath(trimmed.isEmpty ? "." : trimmed)
        if let defaultRoot = defaultToolRoot?
            .trimmingCharacters(in: .whitespacesAndNewlines), !defaultRoot.isEmpty {
            let normalizedDefaultRoot = try projectRelativePath(defaultRoot)
            guard normalizedDefaultRoot != "." else { return path }
            if path == "." { path = normalizedDefaultRoot }
            else if path != normalizedDefaultRoot, !path.hasPrefix(normalizedDefaultRoot + "/") {
                path = normalizedDefaultRoot + "/" + path
            }
        }
        _ = try normalizedComponents(path)
        return path
    }

    private func projectRelativePath(_ rawPath: String) throws -> String {
        var path = rawPath
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "\\", with: "/")
        guard !path.isEmpty, !path.contains("\0"), !looksLikeWindowsAbsolute(path) else {
            throw CodeReadWorkerCoreError.pathOutsideProject
        }
        if path.hasPrefix("/") {
            let requested = URL(fileURLWithPath: path).standardizedFileURL
                .resolvingSymlinksInPath()
            let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
            guard requested.path == root.path || requested.path.hasPrefix(prefix) else {
                throw CodeReadWorkerCoreError.pathOutsideProject
            }
            path = requested.path == root.path
                ? "."
                : String(requested.path.dropFirst(prefix.count))
        } else if let cwd = normalizedRequestCWD {
            if path == cwd { path = "." }
            else if path.hasPrefix(cwd + "/") {
                path = String(path.dropFirst(cwd.count + 1))
            }
        }
        return path
    }

    private var normalizedRequestCWD: String? {
        requestCWD?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "\\", with: "/")
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            .nilIfEmpty
            .flatMap { $0 == "." ? nil : $0 }
    }

    private func resolveExisting(_ path: String) throws -> URL {
        let components = try normalizedComponents(path)
        let candidate = components.reduce(root) { $0.appendingPathComponent($1) }
        guard FileManager.default.fileExists(atPath: candidate.path) else {
            throw CodeReadWorkerCoreError.notFound
        }
        let resolved = candidate.resolvingSymlinksInPath().standardizedFileURL
        guard contains(resolved) else { throw CodeReadWorkerCoreError.pathOutsideProject }
        return resolved
    }

    private func normalizedComponents(_ path: String) throws -> [String] {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.hasPrefix("/"), !trimmed.contains("\0") else {
            throw CodeReadWorkerCoreError.pathOutsideProject
        }
        var result: [String] = []
        for component in trimmed.split(separator: "/", omittingEmptySubsequences: true) {
            let value = String(component)
            if value == "." { continue }
            guard value != ".." else { throw CodeReadWorkerCoreError.pathOutsideProject }
            result.append(value)
        }
        return result
    }

    private func readTextFile(
        path: String
    ) throws -> (byteCount: Int, content: String, sha256: String) {
        try readTextFile(url: resolveExisting(path))
    }

    private func readTextFile(
        url: URL
    ) throws -> (byteCount: Int, content: String, sha256: String) {
        try checkDeadline()
        let descriptor = url.path.withCString {
            Darwin.open($0, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CodeReadWorkerCoreError.notFile }
        defer { _ = Darwin.close(descriptor) }
        var before = stat()
        guard fstat(descriptor, &before) == 0,
              (before.st_mode & S_IFMT) == S_IFREG else {
            throw CodeReadWorkerCoreError.notFile
        }
        guard before.st_size <= ChatOSCodeReadWorkerEngine.maximumReadBytes else {
            throw CodeReadWorkerCoreError.fileTooLarge(Int(before.st_size))
        }
        var data = Data()
        data.reserveCapacity(Int(before.st_size))
        var buffer = [UInt8](repeating: 0, count: 64 * 1_024)
        while true {
            try checkDeadline()
            let count = Darwin.read(descriptor, &buffer, buffer.count)
            if count == 0 { break }
            if count < 0, errno == EINTR { continue }
            guard count > 0,
                  data.count <= ChatOSCodeReadWorkerEngine.maximumReadBytes - count else {
                throw CodeReadWorkerCoreError.fileTooLarge(
                    max(Int(before.st_size), ChatOSCodeReadWorkerEngine.maximumReadBytes + 1)
                )
            }
            data.append(contentsOf: buffer[0..<count])
        }
        var after = stat()
        guard fstat(descriptor, &after) == 0,
              before.st_dev == after.st_dev,
              before.st_ino == after.st_ino,
              before.st_size == after.st_size,
              before.st_mtimespec.tv_sec == after.st_mtimespec.tv_sec,
              before.st_mtimespec.tv_nsec == after.st_mtimespec.tv_nsec else {
            throw CodeReadWorkerCoreError.invalidRequest("读取期间文件身份发生变化")
        }
        guard !data.prefix(8_000).contains(0),
              let content = String(data: data, encoding: .utf8) else {
            throw CodeReadWorkerCoreError.binaryFile
        }
        let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        return (data.count, content, digest)
    }

    private func checkDeadline(_ deadline: Int64? = nil) throws {
        if deadlinePassed(deadline ?? deadlineUnixMilliseconds) {
            throw CodeReadWorkerCoreError.deadlineExceeded
        }
    }

    private func deadlinePassed(_ deadline: Int64) -> Bool {
        Int64(Date().timeIntervalSince1970 * 1_000) >= deadline
    }

    private func operationDeadline(
        _ arguments: [String: CodeReadWorkerJSONValue],
        defaultDurationMilliseconds: Int? = nil
    ) -> Int64 {
        let duration = arguments.number("timeout_ms").map {
            min(max(Int64($0), 50), 60_000)
        } ?? defaultDurationMilliseconds.map(Int64.init)
        guard let duration else { return deadlineUnixMilliseconds }
        let now = Int64(Date().timeIntervalSince1970 * 1_000)
        return min(deadlineUnixMilliseconds, now + duration)
    }

    private func contains(_ url: URL) -> Bool {
        url.path == root.path
            || url.path.hasPrefix(root.path.hasSuffix("/") ? root.path : root.path + "/")
    }

    private func relativePath(_ url: URL) -> String {
        let resolved = url.standardizedFileURL.resolvingSymlinksInPath()
        if resolved.path == root.path { return "." }
        let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        return resolved.path.hasPrefix(prefix) ? String(resolved.path.dropFirst(prefix.count)) : "."
    }

    private func shouldSkip(_ url: URL) -> Bool {
        [".git", ".build", ".cache", ".next", ".idea", ".vscode", "node_modules",
         "DerivedData", "Pods", "target", "dist", "build", "vendor"]
            .contains(url.lastPathComponent)
    }

    private func normalizedLines(_ content: String) -> [String] {
        content.split(separator: "\n", omittingEmptySubsequences: false)
            .map { String($0).trimmingCharacters(in: CharacterSet(charactersIn: "\r")) }
    }

    private func requiredString(
        _ arguments: [String: CodeReadWorkerJSONValue],
        _ key: String
    ) throws -> String {
        guard let value = arguments.string(key)?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            throw CodeReadWorkerCoreError.invalidRequest("缺少参数：\(key)")
        }
        return value
    }

    private func requiredNumber(
        _ arguments: [String: CodeReadWorkerJSONValue],
        _ key: String
    ) throws -> Double {
        guard let value = arguments.number(key) else {
            throw CodeReadWorkerCoreError.invalidRequest("缺少参数：\(key)")
        }
        return value
    }

    private func looksLikeWindowsAbsolute(_ path: String) -> Bool {
        path.utf8.dropFirst().first == Character(":").asciiValue
    }
}

private enum RipgrepSearch {
    static func search(
        executable: URL,
        pattern: String,
        start: URL,
        root: URL,
        maximumResults: Int,
        caseInsensitive: Bool,
        maximumLineCharacters: Int,
        partialOnTimeout: Bool,
        deadlineUnixMilliseconds: Int64
    ) throws -> (results: [CodeReadWorkerJSONValue], truncated: Bool) {
        let relativePath = relative(start, to: root)
        let rg = Process()
        rg.executableURL = executable
        rg.currentDirectoryURL = root
        rg.arguments = [
            "--json", "--fixed-strings", "--hidden", "--no-messages",
            "--max-filesize", String(ChatOSCodeReadWorkerEngine.maximumReadBytes),
            "--max-columns", String(max(1_000, maximumLineCharacters)),
            "--max-columns-preview",
        ] + (caseInsensitive ? ["--ignore-case"] : [])
            + ChatOSCodeReadWorkerEngine.ignoredGlobs.flatMap { ["--glob", $0] }
            + ["--", pattern, relativePath]
        let limiter = Process()
        limiter.executableURL = URL(fileURLWithPath: "/usr/bin/head")
        limiter.arguments = ["-n", String(maximumResults * 4 + 100)]
        let bridge = Pipe()
        let output = Pipe()
        rg.standardInput = FileHandle.nullDevice
        rg.standardOutput = bridge
        rg.standardError = FileHandle.nullDevice
        limiter.standardInput = bridge
        limiter.standardOutput = output
        limiter.standardError = FileHandle.nullDevice
        let capture = BoundedCapture(maximumBytes: 8 * 1_024 * 1_024)
        output.fileHandleForReading.readabilityHandler = { handle in
            capture.append(handle.availableData)
        }
        do {
            try limiter.run()
            try rg.run()
        } catch {
            if limiter.isRunning { _ = Darwin.kill(limiter.processIdentifier, SIGKILL) }
            output.fileHandleForReading.readabilityHandler = nil
            throw CodeReadWorkerCoreError.searchFailed("无法启动项目文本搜索进程")
        }
        defer {
            output.fileHandleForReading.readabilityHandler = nil
            try? output.fileHandleForReading.close()
            if rg.isRunning { _ = Darwin.kill(rg.processIdentifier, SIGKILL) }
            if limiter.isRunning { _ = Darwin.kill(limiter.processIdentifier, SIGKILL) }
        }
        var timedOut = false
        while limiter.isRunning {
            if Int64(Date().timeIntervalSince1970 * 1_000) >= deadlineUnixMilliseconds {
                _ = Darwin.kill(rg.processIdentifier, SIGTERM)
                _ = Darwin.kill(limiter.processIdentifier, SIGTERM)
                timedOut = true
                break
            }
            usleep(5_000)
        }
        if timedOut, !partialOnTimeout {
            throw CodeReadWorkerCoreError.deadlineExceeded
        }
        let rgDeadline = Date().addingTimeInterval(0.5)
        while rg.isRunning, Date() < rgDeadline { usleep(5_000) }
        if rg.isRunning { _ = Darwin.kill(rg.processIdentifier, SIGKILL) }
        capture.append(output.fileHandleForReading.readDataToEndOfFile())
        let snapshot = capture.snapshot
        var matches: [CodeReadWorkerJSONValue] = []
        for line in String(decoding: snapshot.data, as: UTF8.self).split(separator: "\n") {
            guard matches.count < maximumResults,
                  let json = try? JSONDecoder().decode(
                    CodeReadWorkerJSONValue.self,
                    from: Data(line.utf8)
                  ),
                  case let .object(rootObject) = json,
                  rootObject.string("type") == "match",
                  case let .object(data)? = rootObject["data"],
                  case let .object(path)? = data["path"],
                  let rawPath = path.string("text"),
                  let lineNumber = data.number("line_number"),
                  case let .object(lines)? = data["lines"] else { continue }
            let text = (lines.string("text") ?? "").trimmingCharacters(in: .newlines)
            let normalizedPath = rawPath.hasPrefix("./") ? String(rawPath.dropFirst(2)) : rawPath
            let column = firstMatchColumn(data: data, text: text)
            matches.append(.object([
                "path": .string(normalizedPath),
                "line": .number(lineNumber),
                "column": .number(Double(column)),
                "text": .string(String(text.prefix(maximumLineCharacters))),
            ]))
        }
        return (
            matches,
            snapshot.discarded || matches.count >= maximumResults || timedOut
        )
    }

    private static func relative(_ url: URL, to root: URL) -> String {
        if url.path == root.path { return "." }
        let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        return url.path.hasPrefix(prefix) ? String(url.path.dropFirst(prefix.count)) : "."
    }

    private static func firstMatchColumn(
        data: [String: CodeReadWorkerJSONValue],
        text: String
    ) -> Int {
        guard case let .array(submatches)? = data["submatches"],
              case let .object(first)? = submatches.first,
              let byteOffset = first.number("start"), byteOffset >= 0 else { return 1 }
        let prefix = text.utf8.prefix(Int(byteOffset))
        return String(decoding: prefix, as: UTF8.self).count + 1
    }
}

private final class BoundedCapture: @unchecked Sendable {
    private let lock = NSLock()
    private let maximumBytes: Int
    private var data = Data()
    private var discarded = false

    init(maximumBytes: Int) { self.maximumBytes = maximumBytes }

    func append(_ value: Data) {
        guard !value.isEmpty else { return }
        lock.withLock {
            let available = max(0, maximumBytes - data.count)
            data.append(value.prefix(available))
            if value.count > available { discarded = true }
        }
    }

    var snapshot: (data: Data, discarded: Bool) {
        lock.withLock { (data, discarded) }
    }
}

private struct Entry {
    let name: String
    let path: String
    let isDirectory: Bool
    let size: Int64
    let modifiedAt: Int64?

    var jsonValue: CodeReadWorkerJSONValue {
        .object([
            "name": .string(name),
            "path": .string(path),
            "type": .string(isDirectory ? "dir" : "file"),
            "size": .number(Double(size)),
            "mtime_ms": modifiedAt.map { .number(Double($0)) } ?? .null,
        ])
    }
}

private struct WorkspaceEntry {
    let name: String
    let path: String
    let isDirectory: Bool
    let size: Int64
    let modifiedAt: Int64?

    var jsonValue: CodeReadWorkerJSONValue {
        .object([
            "name": .string(name),
            "path": .string(path),
            "is_dir": .bool(isDirectory),
            "size": .number(Double(size)),
            "modified_at": modifiedAt.map { .number(Double($0)) } ?? .null,
        ])
    }
}

private extension Dictionary where Key == String, Value == CodeReadWorkerJSONValue {
    func string(_ key: String) -> String? {
        guard case let .string(value)? = self[key] else { return nil }
        return value
    }

    func number(_ key: String) -> Double? {
        guard case let .number(value)? = self[key] else { return nil }
        return value
    }

    func bool(_ key: String) -> Bool? {
        guard case let .bool(value)? = self[key] else { return nil }
        return value
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
