import ChatOSCore
import Foundation

struct NativeMCPRemoteConnectionController: Sendable {
    static let toolNames: Set<String> = [
        "list_connections", "test_connection", "run_command", "list_directory",
        "read_file", "download_file", "upload_file",
    ]

    static let toolDefinitions: [NativeJSONValue] = [
        definition(name: "list_connections", description: "列出可用的远程连接。", properties: [:], required: []),
        definition(
            name: "test_connection",
            description: "测试远程连接是否可用。",
            properties: ["connection_id": .object(["type": .string("string")])],
            required: ["connection_id"]
        ),
        definition(
            name: "run_command",
            description: "通过 SSH 在远程主机上执行命令。",
            properties: [
                "connection_id": .object(["type": .string("string")]),
                "command": .object(["type": .string("string")]),
                "timeout_seconds": integer(minimum: 1, maximum: 120),
                "allow_dangerous": .object(["type": .string("boolean")]),
                "max_output_chars": integer(minimum: 1, maximum: 20_000),
            ],
            required: ["connection_id", "command"]
        ),
        definition(
            name: "list_directory",
            description: "列出远程目录内容。",
            properties: [
                "connection_id": .object(["type": .string("string")]),
                "path": .object(["type": .string("string")]),
                "limit": integer(minimum: 1, maximum: 1_000),
            ],
            required: ["connection_id"]
        ),
        definition(
            name: "read_file",
            description: "读取远程文本文件。",
            properties: fileReadProperties(includeEncoding: false),
            required: ["connection_id", "path"]
        ),
        definition(
            name: "download_file",
            description: "通过 SSH 下载远程文件内容。",
            properties: fileReadProperties(includeEncoding: true),
            required: ["connection_id", "path"]
        ),
        definition(
            name: "upload_file",
            description: "通过 SSH 上传内容到远程文件。",
            properties: [
                "connection_id": .object(["type": .string("string")]),
                "path": .object(["type": .string("string")]),
                "content": .object(["type": .string("string")]),
                "encoding": .object(["type": .string("string"), "enum": .array([.string("text"), .string("base64")])]),
                "create_parent_dirs": .object(["type": .string("boolean")]),
                "overwrite": .object(["type": .string("boolean")]),
            ],
            required: ["connection_id", "path", "content"]
        ),
    ]

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
        case "list_connections": return try await listConnections()
        case "test_connection": return try await testConnection(arguments)
        case "run_command": return try await runCommand(arguments)
        case "list_directory": return try await listDirectory(arguments)
        case "read_file": return try await readFile(arguments, includeEncoding: false)
        case "download_file": return try await readFile(arguments, includeEncoding: true)
        case "upload_file": return try await uploadFile(arguments)
        default: throw NativeMCPRemoteConnectionError.unsupportedTool(name)
        }
    }

    private func listConnections() async throws -> NativeJSONValue {
        let connections = try await provider.listConnections()
        return .object([
            "connections": .array(connections.map { connection in
                .object([
                    "id": .string(connection.id),
                    "name": .string(connection.name),
                    "host": .string(connection.host),
                    "port": .number(Double(connection.port)),
                    "username": .string(connection.username),
                    "authentication_type": .string(connection.authenticationType.rawValue),
                    "credentials_available": .bool(credentialsAvailable(connection)),
                    "default_remote_path": connection.defaultRemotePath.map(NativeJSONValue.string) ?? .null,
                    "jump_enabled": .bool(connection.jumpEnabled),
                    "last_active_at": connection.lastActiveAt.map { .string(ISO8601DateFormatter().string(from: $0)) } ?? .null,
                ])
            }),
            "count": .number(Double(connections.count)),
        ])
    }

    private func testConnection(_ arguments: [String: NativeJSONValue]) async throws -> NativeJSONValue {
        let id = try requiredString(arguments, "connection_id")
        let result = try await provider.testSaved(id: id, verificationCode: nil)
        return .object([
            "connection_id": .string(id),
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
            "connection_id": .string(id),
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
        return .object([
            "connection_id": .string(id),
            "path": .string(path),
            "entries": .array(entries.map { entry in
                .object([
                    "name": .string(entry.name),
                    "path": .string(entry.path),
                    "type": .string(entry.type),
                    "is_directory": .bool(entry.type == "directory"),
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
            "connection_id": .string(id),
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
            "connection_id": .string(id),
            "path": .string(path),
            "encoding": .string(encoding),
            "size_bytes": .number(Double(data.count)),
            "uploaded": .bool(true),
        ])
    }

    private func credentialsAvailable(_ connection: RemoteConnection) -> Bool {
        switch connection.authenticationType {
        case .password: connection.hasPassword
        case .privateKey: connection.hasPrivateKeyPath
        case .privateKeyCertificate: connection.hasPrivateKeyPath && connection.hasCertificatePath
        }
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

    private static func fileReadProperties(includeEncoding: Bool) -> [String: NativeJSONValue] {
        var values: [String: NativeJSONValue] = [
            "connection_id": .object(["type": .string("string")]),
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
        .object([
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
        .object([
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

private enum NativeMCPRemoteConnectionError: LocalizedError {
    case unsupportedTool(String)
    case invalidArguments(String)
    case dangerousCommand
    case binaryRequiresBase64

    var errorDescription: String? {
        switch self {
        case let .unsupportedTool(name): "不支持的远程连接工具：\(name)"
        case let .invalidArguments(message): message
        case .dangerousCommand: "该远程命令风险较高，需要明确设置 allow_dangerous=true"
        case .binaryRequiresBase64: "远程文件不是 UTF-8 文本，请使用 base64 编码下载"
        }
    }
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
