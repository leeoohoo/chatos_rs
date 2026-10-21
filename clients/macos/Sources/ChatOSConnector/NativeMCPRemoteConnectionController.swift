import ChatOSCore
import Foundation

struct NativeMCPRemoteConnectionController: Sendable {
    static let providerID = ProductToolProviderID.remoteConnection
    static let skillBindingID = ProductToolSkillBindingID.remoteConnection

    static let toolNames: Set<String> = [
        "test_connection", "run_command", "list_directory",
        "read_file", "download_file", "upload_file", "upload_project_file",
    ]

    static let toolDefinitions: [NativeJSONValue] = [
        definition(
            name: "test_connection",
            description: "测试当前选中的远程连接是否可用。",
            properties: [:],
            required: []
        ),
        definition(
            name: "run_command",
            description: "通过 SSH 在远程主机上执行命令。",
            properties: [
                "command": .object(["type": .string("string")]),
                "timeout_seconds": integer(minimum: 1, maximum: 120),
                "allow_dangerous": .object(["type": .string("boolean")]),
                "max_output_chars": integer(minimum: 1, maximum: 20_000),
            ],
            required: ["command"]
        ),
        definition(
            name: "list_directory",
            description: "列出远程目录内容。",
            properties: [
                "path": .object(["type": .string("string")]),
                "limit": integer(minimum: 1, maximum: 1_000),
            ],
            required: []
        ),
        definition(
            name: "read_file",
            description: "读取远程文本文件。",
            properties: fileReadProperties(includeEncoding: false),
            required: ["path"]
        ),
        definition(
            name: "download_file",
            description: "通过 SSH 下载远程文件内容。",
            properties: fileReadProperties(includeEncoding: true),
            required: ["path"]
        ),
        definition(
            name: "upload_file",
            description: "通过 SSH 上传少量文本或 Base64 内容到远程文件。项目中的现有文件必须使用 upload_project_file，禁止先把文件编码后复制进参数。",
            properties: [
                "path": .object(["type": .string("string")]),
                "content": .object(["type": .string("string")]),
                "encoding": .object(["type": .string("string"), "enum": .array([.string("text"), .string("base64")])]),
                "create_parent_dirs": .object(["type": .string("boolean")]),
                "overwrite": .object(["type": .string("boolean")]),
            ],
            required: ["path", "content"]
        ),
        definition(
            name: "upload_project_file",
            description: "直接把当前项目工作区中的现有文件通过 SSH 上传到远端，不把文件内容经过模型上下文。适用于构建产物、压缩包和其他项目文件。",
            properties: [
                "local_path": .object([
                    "type": .string("string"),
                    "description": .string("当前项目根目录内的相对文件路径"),
                ]),
                "path": .object([
                    "type": .string("string"),
                    "description": .string("远端目标路径"),
                ]),
                "overwrite": .object(["type": .string("boolean")]),
            ],
            required: ["local_path", "path"]
        ),
    ]

    static func skillCoverageReport() throws -> ToolSkillCoverageReport {
        let tools = try toolDefinitions.map { definition -> ToolSkillCoverageInput in
            guard let name = definition.jsonObject?["name"]?.jsonString else {
                throw NativeMCPRemoteConnectionError.invalidToolDefinition
            }
            return .init(
                providerID: providerID,
                toolName: name,
                skillBindingID: skillBindingID
            )
        }
        return ToolSkillCoverageCatalog.product.audit(tools)
    }

    let provider: any NativeRemoteConnectionRuntimeProviding
    let ssh: any NativeRemoteSSHExecuting

    init(
        provider: any NativeRemoteConnectionRuntimeProviding,
        ssh: any NativeRemoteSSHExecuting = NativeOpenSSHClient()
    ) {
        self.provider = provider
        self.ssh = ssh
    }

    func call(name: String, arguments: [String: NativeJSONValue]) async throws -> NativeJSONValue {
        switch name {
        case "test_connection": return try await testConnection(arguments)
        case "run_command": return try await runCommand(arguments)
        case "list_directory": return try await listDirectory(arguments)
        case "read_file": return try await readFile(arguments, includeEncoding: false)
        case "download_file": return try await readFile(arguments, includeEncoding: true)
        case "upload_file": return try await uploadFile(arguments)
        case "upload_project_file":
            throw NativeMCPRemoteConnectionError.projectContextRequired
        default: throw NativeMCPRemoteConnectionError.unsupportedTool(name)
        }
    }

    func uploadProjectFile(
        _ arguments: [String: NativeJSONValue],
        projectRoot: URL
    ) async throws -> NativeJSONValue {
        let id = try requiredString(arguments, "connection_id")
        let localPath = try requiredString(arguments, "local_path")
        let remotePath = try requiredString(arguments, "path")
        let localURL = try Self.resolveProjectFile(localPath, root: projectRoot)
        try await ssh.uploadFile(
            draft: provider.resolvedDraft(id: id),
            localURL: localURL,
            remotePath: remotePath,
            overwrite: arguments.bool("overwrite") ?? true
        )
        let size = try localURL.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
        return .object([
            "local_path": .string(localPath),
            "path": .string(remotePath),
            "size_bytes": .number(Double(size)),
            "uploaded": .bool(true),
        ])
    }

    private func testConnection(_ arguments: [String: NativeJSONValue]) async throws -> NativeJSONValue {
        let id = try requiredString(arguments, "connection_id")
        let result = try await provider.testSaved(id: id, verificationCode: nil)
        return .object([
            "success": .bool(result.success),
            "message": result.message.map(NativeJSONValue.string) ?? .null,
        ])
    }

    private func runCommand(_ arguments: [String: NativeJSONValue]) async throws -> NativeJSONValue {
        let id = try requiredString(arguments, "connection_id")
        let command = try requiredString(arguments, "command")
        if Self.isDangerous(command), arguments.bool("allow_dangerous") != true {
            throw NativeMCPRemoteConnectionError.dangerousCommand
        }
        let result = try await ssh.runCommand(
            draft: provider.resolvedDraft(id: id),
            command: command,
            timeoutSeconds: clamp(arguments.integer("timeout_seconds") ?? 20, 1, 120),
            maximumOutputCharacters: clamp(arguments.integer("max_output_chars") ?? 20_000, 1, 20_000)
        )
        return .object([
            "command": .string(command),
            "exit_code": .number(Double(result.exitCode)),
            "success": .bool(result.exitCode == 0 && !result.timedOut),
            "stdout": .string(result.stdout),
            "stderr": .string(result.stderr),
            "truncated": .bool(result.truncated),
            "timed_out": .bool(result.timedOut),
        ])
    }

    private func listDirectory(_ arguments: [String: NativeJSONValue]) async throws -> NativeJSONValue {
        let id = try requiredString(arguments, "connection_id")
        let draft = try await provider.resolvedDraft(id: id)
        let path = arguments.string("path")?.trimmedNonEmpty ?? draft.defaultRemotePath?.trimmedNonEmpty ?? "."
        let entries = try await ssh.listDirectory(
            draft: draft,
            path: path,
            limit: clamp(arguments.integer("limit") ?? 200, 1, 1_000)
        )
        let dateFormatter = ISO8601DateFormatter()
        return .object([
            "path": .string(path),
            "entries": .array(entries.map { entry in
                .object([
                    "name": .string(entry.name),
                    "path": .string(entry.path),
                    "type": .string(entry.type),
                    "is_directory": .bool(entry.type == "directory"),
                    "size_bytes": entry.size.map { .number(Double($0)) } ?? .null,
                    "modified_at": entry.modifiedAt.map {
                        .string(dateFormatter.string(from: $0))
                    } ?? .null,
                    "permissions": entry.permissions.map(NativeJSONValue.string) ?? .null,
                ])
            }),
            "count": .number(Double(entries.count)),
        ])
    }

    private func readFile(
        _ arguments: [String: NativeJSONValue],
        includeEncoding: Bool
    ) async throws -> NativeJSONValue {
        let id = try requiredString(arguments, "connection_id")
        let path = try requiredString(arguments, "path")
        let encoding = includeEncoding ? (arguments.string("encoding") ?? "text") : "text"
        guard encoding == "text" || encoding == "base64" else {
            throw NativeMCPRemoteConnectionError.invalidArguments("encoding 仅支持 text 或 base64")
        }
        let data = try await ssh.download(
            draft: provider.resolvedDraft(id: id),
            path: path,
            maximumBytes: clamp(arguments.integer("max_bytes") ?? 256 * 1_024, 1, 256 * 1_024)
        )
        let content: String
        if encoding == "base64" {
            content = data.base64EncodedString()
        } else {
            guard let text = String(data: data, encoding: .utf8), !data.prefix(8_000).contains(0) else {
                throw NativeMCPRemoteConnectionError.binaryRequiresBase64
            }
            content = text
        }
        return .object([
            "path": .string(path),
            "encoding": .string(encoding),
            "size_bytes": .number(Double(data.count)),
            "content": .string(content),
        ])
    }

    private func uploadFile(_ arguments: [String: NativeJSONValue]) async throws -> NativeJSONValue {
        let id = try requiredString(arguments, "connection_id")
        let path = try requiredString(arguments, "path")
        guard case let .string(content)? = arguments["content"] else {
            throw NativeMCPRemoteConnectionError.invalidArguments("缺少参数：content")
        }
        let encoding = arguments.string("encoding") ?? "text"
        let data: Data
        if encoding == "base64" {
            guard let decoded = Data(base64Encoded: content) else {
                throw NativeMCPRemoteConnectionError.invalidArguments("content 不是有效的 Base64")
            }
            data = decoded
        } else if encoding == "text" {
            data = Data(content.utf8)
        } else {
            throw NativeMCPRemoteConnectionError.invalidArguments("encoding 仅支持 text 或 base64")
        }
        try await ssh.upload(
            draft: provider.resolvedDraft(id: id),
            path: path,
            data: data,
            createParentDirectories: arguments.bool("create_parent_dirs") ?? true,
            overwrite: arguments.bool("overwrite") ?? true
        )
        return .object([
            "path": .string(path),
            "encoding": .string(encoding),
            "size_bytes": .number(Double(data.count)),
            "uploaded": .bool(true),
        ])
    }

    private func requiredString(_ values: [String: NativeJSONValue], _ key: String) throws -> String {
        guard let value = values.string(key)?.trimmedNonEmpty else {
            throw NativeMCPRemoteConnectionError.invalidArguments("缺少参数：\(key)")
        }
        return value
    }

    private func clamp(_ value: Int, _ minimum: Int, _ maximum: Int) -> Int {
        min(max(value, minimum), maximum)
    }

    private static func isDangerous(_ command: String) -> Bool {
        let normalized = command.lowercased()
        return normalized.contains("rm -rf /")
            || normalized.contains("mkfs")
            || normalized.contains("shutdown")
            || normalized.contains("reboot")
            || normalized.contains(":(){ :|:& };:")
    }

    private static func resolveProjectFile(_ path: String, root: URL) throws -> URL {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty,
              !trimmed.hasPrefix("/"),
              !trimmed.contains("\0") else {
            throw NativeMCPRemoteConnectionError.invalidArguments(
                "local_path 必须是当前项目内的相对文件路径"
            )
        }
        let components = trimmed.split(separator: "/", omittingEmptySubsequences: true)
        guard !components.isEmpty,
              components.allSatisfy({ $0 != "." && $0 != ".." }) else {
            throw NativeMCPRemoteConnectionError.invalidArguments(
                "local_path 必须是当前项目内的规范相对路径"
            )
        }
        let canonicalRoot = root.standardizedFileURL.resolvingSymlinksInPath()
        let candidate = components.reduce(canonicalRoot) {
            $0.appendingPathComponent(String($1), isDirectory: false)
        }
        let resolved = candidate.standardizedFileURL.resolvingSymlinksInPath()
        let prefix = canonicalRoot.path.hasSuffix("/")
            ? canonicalRoot.path
            : canonicalRoot.path + "/"
        let values = try? resolved.resourceValues(forKeys: [
            .isRegularFileKey, .isReadableKey,
        ])
        guard resolved.path.hasPrefix(prefix),
              values?.isRegularFile == true,
              values?.isReadable == true else {
            throw NativeMCPRemoteConnectionError.invalidArguments(
                "local_path 不存在、不可读或超出当前项目"
            )
        }
        return resolved
    }

    private static func fileReadProperties(includeEncoding: Bool) -> [String: NativeJSONValue] {
        var values: [String: NativeJSONValue] = [
            "path": .object(["type": .string("string")]),
            "max_bytes": integer(minimum: 1, maximum: 262_144),
        ]
        if includeEncoding {
            values["encoding"] = .object([
                "type": .string("string"),
                "enum": .array([.string("text"), .string("base64")]),
            ])
        }
        return values
    }

    private static func integer(minimum: Int, maximum: Int) -> NativeJSONValue {
        return .object([
            "type": .string("integer"),
            "minimum": .number(Double(minimum)),
            "maximum": .number(Double(maximum)),
        ])
    }

    private static func definition(
        name: String,
        description: String,
        properties: [String: NativeJSONValue],
        required: [String]
    ) -> NativeJSONValue {
        return .object([
            "name": .string(name),
            "description": .string(description),
            "inputSchema": .object([
                "type": .string("object"),
                "properties": .object(properties),
                "required": .array(required.map(NativeJSONValue.string)),
                "additionalProperties": .bool(false),
            ]),
        ])
    }
}

enum NativeMCPRemoteConnectionError: LocalizedError {
    case unsupportedTool(String)
    case invalidToolDefinition
    case invalidArguments(String)
    case dangerousCommand
    case binaryRequiresBase64
    case projectContextRequired

    var errorDescription: String? {
        switch self {
        case let .unsupportedTool(name): "不支持的远程连接工具：\(name)"
        case .invalidToolDefinition: "远程连接工具定义缺少名称"
        case let .invalidArguments(message): message
        case .dangerousCommand: "该远程命令风险较高，需要明确设置 allow_dangerous=true"
        case .binaryRequiresBase64: "远程文件不是 UTF-8 文本，请使用 base64 编码下载"
        case .projectContextRequired: "upload_project_file 只能由任务工作区调用"
        }
    }

    var isKnownPreEffectFailure: Bool { true }
}

private extension Dictionary where Key == String, Value == NativeJSONValue {
    func string(_ key: String) -> String? {
        guard case let .string(value)? = self[key] else { return nil }
        return value
    }

    func bool(_ key: String) -> Bool? {
        guard case let .bool(value)? = self[key] else { return nil }
        return value
    }

    func integer(_ key: String) -> Int? {
        guard case let .number(value)? = self[key] else { return nil }
        return Int(value)
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
