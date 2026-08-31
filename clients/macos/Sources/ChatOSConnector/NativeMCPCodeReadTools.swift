import ChatOSCore
import CryptoKit
import Foundation

struct NativeMCPCodeReadTools: Sendable {
    private static let maximumReadBytes = 2 * 1_024 * 1_024
    private static let ignoredGlobs = [
        "!.git/**", "!.build/**", "!.cache/**", "!.next/**", "!.idea/**",
        "!.vscode/**", "!node_modules/**", "!DerivedData/**", "!Pods/**",
        "!target/**", "!dist/**", "!build/**", "!vendor/**",
    ]

    let workspace: LocalConnectorWorkspace
    let projectRoot: URL
    let requestCWD: String?
    let defaultToolRoot: String?

    static var toolDefinitions: [NativeJSONValue] {
        [
            definition(
                name: "read_file_raw",
                description: "读取当前本机项目中的 UTF-8 文本文件。",
                properties: [
                    "path": .object(["type": .string("string")]),
                    "with_line_numbers": .object([
                        "type": .string("boolean"), "default": .bool(true),
                    ]),
                ],
                required: ["path"]
            ),
            definition(
                name: "read_file_range",
                description: "按 1-based 行号读取当前本机项目中的文本片段。",
                properties: [
                    "path": .object(["type": .string("string")]),
                    "start_line": .object(["type": .string("integer"), "minimum": .number(1)]),
                    "end_line": .object(["type": .string("integer"), "minimum": .number(1)]),
                    "with_line_numbers": .object(["type": .string("boolean")]),
                ],
                required: ["path", "start_line", "end_line"]
            ),
            definition(
                name: "list_dir",
                description: "列出当前本机项目中的目录内容。",
                properties: [
                    "path": .object(["type": .string("string")]),
                    "max_entries": .object([
                        "type": .string("integer"), "minimum": .number(1), "maximum": .number(1_000),
                    ]),
                ],
                required: []
            ),
            definition(
                name: "search_text",
                description: "使用客户端内置 ripgrep 在当前本机项目中递归搜索固定文本。",
                properties: [
                    "pattern": .object(["type": .string("string"), "minLength": .number(1)]),
                    "path": .object(["type": .string("string")]),
                    "max_results": .object([
                        "type": .string("integer"), "minimum": .number(1), "maximum": .number(500),
                    ]),
                ],
                required: ["pattern"]
            ),
            definition(
                name: "read_file",
                description: "read_file_raw/read_file_range 的兼容别名。",
                properties: [
                    "path": .object(["type": .string("string")]),
                    "start_line": .object(["type": .string("integer"), "minimum": .number(1)]),
                    "end_line": .object(["type": .string("integer"), "minimum": .number(1)]),
                    "with_line_numbers": .object([
                        "type": .string("boolean"), "default": .bool(true),
                    ]),
                ],
                required: ["path"]
            ),
            definition(
                name: "search_files",
                description: "search_text 的兼容别名，query 会映射到 pattern。",
                properties: [
                    "query": .object(["type": .string("string"), "minLength": .number(1)]),
                    "path": .object(["type": .string("string")]),
                    "max_results": .object([
                        "type": .string("integer"), "minimum": .number(1), "maximum": .number(500),
                    ]),
                ],
                required: ["query"]
            ),
        ]
    }

    func call(name: String, arguments: [String: NativeJSONValue]) throws -> NativeJSONValue {
        switch name {
        case "read_file_raw":
            return try readFile(arguments)
        case "read_file_range":
            return try readRange(arguments)
        case "list_dir":
            return try listDirectory(arguments)
        case "search_text":
            return try searchText(arguments)
        case "read_file":
            if arguments.number("start_line") != nil || arguments.number("end_line") != nil {
                guard arguments.number("start_line") != nil, arguments.number("end_line") != nil else {
                    throw NativeMCPCodeReadError.invalidArguments("start_line 与 end_line 必须同时提供")
                }
                return try readRange(arguments)
            }
            return try readFile(arguments)
        case "search_files":
            var mapped = arguments
            if mapped["pattern"] == nil, let query = arguments.string("query") {
                mapped["pattern"] = .string(query)
            }
            return try searchText(mapped)
        default:
            throw NativeMCPCodeReadError.unsupportedTool(name)
        }
    }

    private func readFile(_ arguments: [String: NativeJSONValue]) throws -> NativeJSONValue {
        let path = try normalizedToolPath(requiredString(arguments, "path"))
        let file = try readTextFile(path: path)
        let lines = normalizedLines(file.content)
        var payload: [String: NativeJSONValue] = [
            "path": .string(path),
            "size_bytes": .number(Double(file.data.count)),
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

    private func readRange(_ arguments: [String: NativeJSONValue]) throws -> NativeJSONValue {
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
            "size_bytes": .number(Double(file.data.count)),
            "sha256": .string(file.sha256),
            "start_line": .number(Double(start)),
            "end_line": .number(Double(end)),
            "total_lines": .number(Double(lines.count)),
            "content": .string(selected.joined(separator: "\n")),
        ])
    }

    private func listDirectory(_ arguments: [String: NativeJSONValue]) throws -> NativeJSONValue {
        let path = try normalizedToolPath(arguments.string("path") ?? ".")
        let maximum = min(max(Int(arguments.number("max_entries") ?? 200), 1), 1_000)
        let listed = try filesystem.list(path: path, includeFiles: true).objectValue()
        let entries = try listed.arrayValue("entries").prefix(maximum).map { value -> NativeJSONValue in
            let entry = try value.objectValue()
            let isDirectory = entry.boolValue("is_dir") ?? false
            return .object([
                "name": entry["name"] ?? .string(""),
                "path": entry["path"] ?? .string(""),
                "type": .string(isDirectory ? "dir" : "file"),
                "size": entry["size"] ?? .number(0),
                "mtime_ms": entry["modified_at"] ?? .number(0),
            ])
        }
        return .object(["entries": .array(entries)])
    }

    private func searchText(_ arguments: [String: NativeJSONValue]) throws -> NativeJSONValue {
        let pattern = try requiredString(arguments, "pattern")
        let path = try normalizedToolPath(arguments.string("path") ?? ".")
        let maximum = min(max(Int(arguments.number("max_results") ?? 200), 1), 500)
        let start = try filesystem.resolveExistingURL(path)

        let results: [NativeJSONValue]
        if let ripgrep = NativeBundledToolLocator.executable(named: "rg") {
            results = try NativeRipgrepSearch.search(
                executable: ripgrep,
                pattern: pattern,
                start: start,
                projectRoot: projectRoot,
                maximumResults: maximum,
                ignoredGlobs: Self.ignoredGlobs,
                maximumFileBytes: Self.maximumReadBytes
            )
        } else {
            let fallback = try filesystem.searchContent(path: path, query: pattern, limit: maximum)
            results = try fallback.objectValue().arrayValue("matches")
        }
        return .object([
            "count": .number(Double(results.count)),
            "results": .array(results),
        ])
    }

    private var filesystem: NativeWorkspaceFilesystem {
        NativeWorkspaceFilesystem(workspace: .init(
            id: workspace.id,
            alias: workspace.alias,
            absoluteRoot: projectRoot.path,
            fingerprint: workspace.fingerprint,
            projectConfigTrusted: workspace.projectConfigTrusted,
            projectConfigTrustStale: workspace.projectConfigTrustStale
        ))
    }

    private func normalizedToolPath(_ rawPath: String) throws -> String {
        var path = try projectRelativePath(rawPath)
        if let defaultRoot = defaultToolRoot?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .nilIfEmpty {
            let normalizedDefaultRoot = try projectRelativePath(defaultRoot)
            guard normalizedDefaultRoot != "." else { return path }
            if path == "." { path = normalizedDefaultRoot }
            else if path != normalizedDefaultRoot, !path.hasPrefix(normalizedDefaultRoot + "/") {
                path = normalizedDefaultRoot + "/" + path
            }
        }
        return path
    }

    private func projectRelativePath(_ rawPath: String) throws -> String {
        var path = rawPath
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "\\", with: "/")
        guard !path.isEmpty, !path.contains("\0"), !looksLikeWindowsAbsolute(path) else {
            throw NativeMCPCodeReadError.pathOutsideProject
        }

        if path.hasPrefix("/") {
            let root = projectRoot.standardizedFileURL.resolvingSymlinksInPath()
            let requested = URL(fileURLWithPath: path)
                .standardizedFileURL
                .resolvingSymlinksInPath()
            let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
            guard requested.path == root.path || requested.path.hasPrefix(prefix) else {
                throw NativeMCPCodeReadError.pathOutsideProject
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

    private func readTextFile(path: String) throws -> (data: Data, content: String, sha256: String) {
        let url = try filesystem.resolveExistingURL(path)
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard values.isRegularFile == true else { throw NativeMCPCodeReadError.notFile }
        let size = values.fileSize ?? Self.maximumReadBytes + 1
        guard size <= Self.maximumReadBytes else { throw NativeMCPCodeReadError.fileTooLarge(size) }
        let data = try Data(contentsOf: url, options: [.mappedIfSafe])
        guard !data.prefix(8_000).contains(0), let content = String(data: data, encoding: .utf8) else {
            throw NativeMCPCodeReadError.binaryFile
        }
        let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        return (data, content, digest)
    }

    private func normalizedLines(_ content: String) -> [String] {
        content.split(separator: "\n", omittingEmptySubsequences: false)
            .map { String($0).trimmingCharacters(in: CharacterSet(charactersIn: "\r")) }
    }

    private func requiredString(
        _ arguments: [String: NativeJSONValue],
        _ key: String
    ) throws -> String {
        guard let value = arguments.string(key)?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else { throw NativeMCPCodeReadError.invalidArguments("缺少参数：\(key)") }
        return value
    }

    private func requiredNumber(
        _ arguments: [String: NativeJSONValue],
        _ key: String
    ) throws -> Double {
        guard let value = arguments.number(key) else {
            throw NativeMCPCodeReadError.invalidArguments("缺少参数：\(key)")
        }
        return value
    }

    private func looksLikeWindowsAbsolute(_ path: String) -> Bool {
        path.utf8.dropFirst().first == Character(":").asciiValue
    }

    private static func definition(
        name: String,
        description: String,
        properties: [String: NativeJSONValue],
        required: [String]
    ) -> NativeJSONValue {
        .object([
            "name": .string(name),
            "description": .string(description),
            "inputSchema": .object([
                "type": .string("object"),
                "properties": .object(properties),
                "additionalProperties": .bool(false),
                "required": .array(required.map(NativeJSONValue.string)),
            ]),
        ])
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}

private enum NativeRipgrepSearch {
    static func search(
        executable: URL,
        pattern: String,
        start: URL,
        projectRoot: URL,
        maximumResults: Int,
        ignoredGlobs: [String],
        maximumFileBytes: Int
    ) throws -> [NativeJSONValue] {
        let relativePath = relative(start, to: projectRoot)
        let rg = Process()
        rg.executableURL = executable
        rg.currentDirectoryURL = projectRoot
        rg.arguments = [
            "--json", "--fixed-strings", "--hidden", "--no-messages",
            "--max-filesize", String(maximumFileBytes),
        ] + ignoredGlobs.flatMap { ["--glob", $0] } + ["--", pattern, relativePath]

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

        try limiter.run()
        try rg.run()
        let data = output.fileHandleForReading.readDataToEndOfFile()
        limiter.waitUntilExit()
        if rg.isRunning { rg.terminate() }
        rg.waitUntilExit()

        var matches: [NativeJSONValue] = []
        for line in String(decoding: data, as: UTF8.self).split(separator: "\n") {
            guard matches.count < maximumResults,
                  let json = try? JSONDecoder().decode(NativeJSONValue.self, from: Data(line.utf8)),
                  case let .object(root) = json,
                  root.stringValue("type") == "match",
                  case let .object(data)? = root["data"],
                  case let .object(path)? = data["path"],
                  let rawPath = path.stringValue("text"),
                  let lineNumber = data.numberValue("line_number"),
                  case let .object(lines)? = data["lines"] else { continue }
            let text = (lines.stringValue("text") ?? "")
                .trimmingCharacters(in: .newlines)
            let normalizedPath = rawPath.hasPrefix("./") ? String(rawPath.dropFirst(2)) : rawPath
            matches.append(.object([
                "path": .string(normalizedPath),
                "line": .number(lineNumber),
                "text": .string(String(text.prefix(400))),
            ]))
        }
        return matches
    }

    private static func relative(_ url: URL, to root: URL) -> String {
        let rootPath = root.standardizedFileURL.path
        let urlPath = url.standardizedFileURL.path
        if urlPath == rootPath { return "." }
        let prefix = rootPath.hasSuffix("/") ? rootPath : rootPath + "/"
        return urlPath.hasPrefix(prefix) ? String(urlPath.dropFirst(prefix.count)) : "."
    }
}

private enum NativeMCPCodeReadError: LocalizedError {
    case invalidArguments(String)
    case pathOutsideProject
    case notFile
    case fileTooLarge(Int)
    case binaryFile
    case unsupportedTool(String)

    var errorDescription: String? {
        switch self {
        case let .invalidArguments(message): message
        case .pathOutsideProject: "路径超出当前项目范围"
        case .notFile: "目标不是普通文件"
        case let .fileTooLarge(size): "文件超过读取限制：\(size) bytes"
        case .binaryFile: "二进制文件不支持按文本读取"
        case let .unsupportedTool(name): "暂不支持 MCP 工具：\(name)"
        }
    }
}

private extension Dictionary where Key == String, Value == NativeJSONValue {
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

private extension NativeJSONValue {
    func objectValue() throws -> [String: NativeJSONValue] {
        guard case let .object(value) = self else {
            throw NativeMCPCodeReadError.invalidArguments("工具返回结构无效")
        }
        return value
    }
}

private extension Dictionary where Key == String, Value == NativeJSONValue {
    func arrayValue(_ key: String) throws -> [NativeJSONValue] {
        guard case let .array(value)? = self[key] else {
            throw NativeMCPCodeReadError.invalidArguments("工具返回缺少 \(key)")
        }
        return value
    }

    func boolValue(_ key: String) -> Bool? { bool(key) }
    func stringValue(_ key: String) -> String? { string(key) }
    func numberValue(_ key: String) -> Double? { number(key) }
}
