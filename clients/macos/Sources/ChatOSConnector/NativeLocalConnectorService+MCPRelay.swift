import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    func handleMCPRelayMessage(
        _ data: Data,
        socket: URLSessionWebSocketTask
    ) async {
        let decoded = try? JSONDecoder().decode(NativeRelayRequest.self, from: data)
        let requestID = decoded?.requestID ?? ""
        do {
            guard let request = decoded else { throw NativeMCPRelayError.invalidRequest }
            let response = try await processMCPRelay(request)
            try await sendRelayResponse(response, socket: socket)
        } catch {
            let response = NativeRelayResponse(
                type: "mcp",
                requestID: requestID,
                status: 400,
                body: Self.rpcError(id: nil, code: -32603, message: error.localizedDescription)
            )
            try? await sendRelayResponse(response, socket: socket)
        }
    }

    private func processMCPRelay(_ request: NativeRelayRequest) async throws -> NativeRelayResponse {
        guard request.type == "mcp",
              let ownerUserID = state.user?.id,
              let deviceID = state.deviceID,
              let workspace = state.workspaces.first(where: { $0.id == request.workspaceID }) else {
            throw NativeMCPRelayError.invalidContext
        }
        let token = try requireAccessToken()
        let runtime = try await gateway.managedRuntimeConfig(token: token)
        try NativeRelayVerifier().verify(
            request,
            trust: runtime.remoteControlTrust,
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            seenNonces: &seenRelayNonces
        )

        if request.header("x-local-connector-inline-mcp-runtime") != nil {
            let body = try await NativeMCPHTTPProxy.forward(request: request)
            return .init(type: "mcp", requestID: request.requestID, status: 200, body: body)
        }

        let call = try NativeMCPRequest(request.body)
        let policy = NativeMCPBuiltinPolicy(
            header: request.header("x-local-connector-enabled-builtin-kinds")
        )
        switch call.method {
        case "initialize":
            return Self.mcpResultResponse(
                requestID: request.requestID,
                rpcID: call.id,
                result: .object([
                    "protocolVersion": .string("2024-11-05"),
                    "capabilities": .object(["tools": .object([:])]),
                    "serverInfo": .object([
                        "name": .string("local_connector"),
                        "version": .string("swift-native"),
                    ]),
                ])
            )
        case "notifications/initialized", "ping":
            return Self.mcpResultResponse(
                requestID: request.requestID,
                rpcID: call.id,
                result: .object([:])
            )
        case "tools/list":
            var tools: [NativeJSONValue] = []
            if policy.codeRead { tools += NativeMCPCodeReadTools.toolDefinitions }
            if policy.codeWrite { tools += NativeMCPCodeWriteStore.toolDefinitions }
            if policy.terminal { tools += NativeMCPTerminalStore.toolDefinitions }
            if policy.remoteConnection, remoteConnectionRuntime != nil {
                tools += NativeMCPRemoteConnectionController.toolDefinitions
            }
            return Self.mcpResultResponse(
                requestID: request.requestID,
                rpcID: call.id,
                result: .object(["tools": .array(tools)])
            )
        case "local_connector/execution_scope/finalize":
            return Self.mcpResultResponse(
                requestID: request.requestID,
                rpcID: call.id,
                result: .object([
                    "status": .string("no_changes"),
                    "files": .array([]),
                    "message": .string("Local Connector tools execute directly in the local project"),
                ])
            )
        case "tools/call":
            break
        default:
            return .init(
                type: "mcp",
                requestID: request.requestID,
                status: 200,
                body: Self.rpcError(id: call.id, code: -32601, message: "暂不支持该 MCP 方法")
            )
        }

        guard let toolName = call.name else {
            return .init(
                type: "mcp",
                requestID: request.requestID,
                status: 200,
                body: Self.rpcError(id: call.id, code: -32602, message: "tools/call.name 不能为空")
            )
        }
        let workspaceRoot = URL(fileURLWithPath: workspace.absoluteRoot)
        let requestCWD = request.header("x-local-connector-cwd")
        let projectRoot = try resolveDirectory(
            requestCWD ?? ".",
            relativeTo: workspaceRoot,
            workspace: workspace
        )
        if Self.codeReadToolNames.contains(toolName) {
            guard policy.codeRead else {
                return .init(
                    type: "mcp",
                    requestID: request.requestID,
                    status: 200,
                    body: Self.rpcError(id: call.id, code: -32601, message: "当前任务未授权代码读取 MCP")
                )
            }
            do {
                let result = try await Task.detached {
                    try NativeMCPCodeReadTools(
                        workspace: workspace,
                        projectRoot: projectRoot,
                        requestCWD: requestCWD,
                        defaultToolRoot: request.header("x-local-connector-default-tool-root")
                    ).call(name: toolName, arguments: call.arguments)
                }.value
                return Self.mcpResultResponse(
                    requestID: request.requestID,
                    rpcID: call.id,
                    result: Self.mcpToolResult(result)
                )
            } catch {
                return .init(
                    type: "mcp",
                    requestID: request.requestID,
                    status: 200,
                    body: Self.rpcError(id: call.id, code: -32603, message: error.localizedDescription)
                )
            }
        }

        if NativeMCPCodeWriteStore.toolNames.contains(toolName) {
            guard policy.codeWrite else {
                return .init(
                    type: "mcp",
                    requestID: request.requestID,
                    status: 200,
                    body: Self.rpcError(id: call.id, code: -32601, message: "当前任务未授权代码修改 MCP")
                )
            }
            let sessionID = request.header("x-mcp-management-session-id")
                ?? request.header("x-mcp-management-run-id")
                ?? request.workspaceID
            let runID = request.header("x-mcp-management-run-id")
                ?? request.header("x-mcp-management-session-id")
                ?? request.workspaceID
            do {
                let result = try await mcpCodeWriteStore.call(
                    name: toolName,
                    arguments: call.arguments,
                    scope: .init(
                        workspaceID: request.workspaceID,
                        sessionID: sessionID,
                        runID: runID
                    ),
                    projectRoot: projectRoot
                )
                return Self.mcpResultResponse(
                    requestID: request.requestID,
                    rpcID: call.id,
                    result: Self.mcpToolResult(result)
                )
            } catch {
                return .init(
                    type: "mcp",
                    requestID: request.requestID,
                    status: 200,
                    body: Self.rpcError(id: call.id, code: -32603, message: error.localizedDescription)
                )
            }
        }

        if NativeMCPRemoteConnectionController.toolNames.contains(toolName) {
            guard policy.remoteConnection, let remoteConnectionRuntime else {
                return .init(
                    type: "mcp",
                    requestID: request.requestID,
                    status: 200,
                    body: Self.rpcError(id: call.id, code: -32601, message: "当前任务无法使用远程连接 MCP")
                )
            }
            do {
                let result = try await NativeMCPRemoteConnectionController(
                    provider: remoteConnectionRuntime
                ).call(name: toolName, arguments: call.arguments)
                return Self.mcpResultResponse(
                    requestID: request.requestID,
                    rpcID: call.id,
                    result: Self.mcpToolResult(result)
                )
            } catch {
                return .init(
                    type: "mcp",
                    requestID: request.requestID,
                    status: 200,
                    body: Self.rpcError(id: call.id, code: -32603, message: error.localizedDescription)
                )
            }
        }

        guard NativeMCPTerminalStore.toolNames.contains(toolName) else {
            return .init(
                type: "mcp",
                requestID: request.requestID,
                status: 200,
                body: Self.rpcError(id: call.id, code: -32601, message: "暂不支持 MCP 工具：\(toolName)")
            )
        }
        guard policy.terminal else {
            return .init(
                type: "mcp",
                requestID: request.requestID,
                status: 200,
                body: Self.rpcError(id: call.id, code: -32601, message: "当前任务未授权终端 MCP")
            )
        }

        if toolName != "execute_command" {
            do {
                let result = try await mcpTerminalStore.call(
                    name: toolName,
                    arguments: call.arguments,
                    projectRoot: projectRoot
                )
                return Self.mcpResultResponse(
                    requestID: request.requestID,
                    rpcID: call.id,
                    result: Self.mcpToolResult(result)
                )
            } catch {
                return .init(
                    type: "mcp",
                    requestID: request.requestID,
                    status: 200,
                    body: Self.rpcError(id: call.id, code: -32603, message: error.localizedDescription)
                )
            }
        }

        let command = call.arguments.string("common")
            ?? call.arguments.string("command")
            ?? ""
        guard !command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw NativeMCPRelayError.emptyCommand
        }
        let cwd = try resolveDirectory(
            call.arguments.string("path") ?? ".",
            relativeTo: projectRoot,
            workspace: workspace
        )
        let shellArguments = ["-lc", command]
        let risk = NativeApprovalRiskEvaluator.evaluate(command: "/bin/zsh", arguments: shellArguments)
        let source = isHarnessImport(command) ? "project-harness-import" : "local-mcp"
        let decision: NativeApprovalDecision
        if source == "project-harness-import" {
            decision = .approve(
                reason: "用户创建本机项目时，由平台签名的 Harness 导入流程发起。",
                rememberAllow: false
            )
        } else {
            decision = await approvalDecision(
                requestID: request.requestID,
                command: "/bin/zsh",
                arguments: shellArguments,
                cwd: cwd,
                projectRoot: projectRoot,
                source: source,
                risk: risk
            )
        }

        switch decision {
        case let .deny(reason), let .askUser(reason):
            appendApprovalHistory(
                command: "/bin/zsh",
                arguments: shellArguments,
                cwd: cwd.path,
                source: source,
                decision: "denied",
                risk: risk,
                reason: reason
            )
            return Self.mcpCommandResponse(
                requestID: request.requestID,
                rpcID: call.id,
                command: command,
                cwd: cwd.path,
                result: nil,
                error: reason
            )
        case let .approve(reason, _):
            appendApprovalHistory(
                command: "/bin/zsh",
                arguments: shellArguments,
                cwd: cwd.path,
                source: source,
                decision: "approved",
                risk: risk,
                reason: reason
            )
        }

        do {
            let structured = try await mcpTerminalStore.execute(
                command: command,
                cwd: cwd,
                projectRoot: projectRoot,
                background: call.arguments.bool("background") ?? false
            )
            if let historyResult = Self.commandHistoryResult(
                structured: structured,
                command: command,
                cwd: cwd.path
            ) {
                appendCommandHistory(
                    result: historyResult,
                    display: command,
                    workspace: workspace,
                    source: source
                )
            }
            return Self.mcpResultResponse(
                requestID: request.requestID,
                rpcID: call.id,
                result: Self.mcpToolResult(structured)
            )
        } catch {
            return .init(
                type: "mcp",
                requestID: request.requestID,
                status: 200,
                body: Self.rpcError(id: call.id, code: -32603, message: error.localizedDescription)
            )
        }
    }

    private func isHarnessImport(_ command: String) -> Bool {
        command.contains("Import local project into ChatOS Harness")
            && command.contains("git push --force origin HEAD:refs/heads/")
            && command.contains("chatos-harness-import-")
    }

    private nonisolated static func mcpCommandResponse(
        requestID: String,
        rpcID: NativeJSONValue,
        command: String,
        cwd: String,
        result: LocalConnectorTerminalResult?,
        error: String?
    ) -> NativeRelayResponse {
        let structured = NativeJSONValue.object([
            "command": .string(command),
            "common": .string(command),
            "path": .string(cwd),
            "success": .bool(result?.success ?? false),
            "exit_code": result?.exitCode.map { .number(Double($0)) } ?? .null,
            "timed_out": .bool(result?.timedOut ?? false),
            "stdout": .string(result?.stdout ?? ""),
            "stderr": .string(result?.stderr ?? ""),
            "error": error.map(NativeJSONValue.string) ?? .null,
        ])
        let toolResult = NativeJSONValue.object([
            "content": .array([
                .object([
                    "type": .string("text"),
                    "text": .string(structured.canonicalJSONString),
                ]),
            ]),
            "_structured_result": structured,
        ])
        return .init(
            type: "mcp",
            requestID: requestID,
            status: 200,
            body: .object([
                "jsonrpc": .string("2.0"),
                "id": rpcID,
                "result": toolResult,
            ])
        )
    }

    private nonisolated static func mcpResultResponse(
        requestID: String,
        rpcID: NativeJSONValue,
        result: NativeJSONValue
    ) -> NativeRelayResponse {
        .init(
            type: "mcp",
            requestID: requestID,
            status: 200,
            body: .object([
                "jsonrpc": .string("2.0"),
                "id": rpcID,
                "result": result,
            ])
        )
    }

    private nonisolated static func commandHistoryResult(
        structured: NativeJSONValue,
        command: String,
        cwd: String
    ) -> LocalConnectorTerminalResult? {
        guard case let .object(values) = structured,
              case let .bool(background)? = values["background"], !background else { return nil }
        return .init(
            command: "/bin/zsh",
            args: ["-lc", command],
            cwd: cwd,
            success: values.bool("success") ?? false,
            exitCode: values.number("exit_code").map(Int.init),
            timedOut: values.string("finished_by") == "timeout",
            stdout: values.string("stdout") ?? "",
            stderr: values.string("stderr") ?? "",
            error: nil
        )
    }

    private nonisolated static func mcpToolResult(_ structured: NativeJSONValue) -> NativeJSONValue {
        .object([
            "content": .array([
                .object([
                    "type": .string("text"),
                    "text": .string(structured.canonicalJSONString),
                ]),
            ]),
            "_structured_result": structured,
        ])
    }

    private nonisolated static var codeReadToolNames: Set<String> {
        [
            "read_file_raw", "read_file_range", "read_file", "list_dir", "search_text",
            "search_files", "open_file_in_pet",
        ]
    }

    private nonisolated static func rpcError(
        id: NativeJSONValue?,
        code: Int,
        message: String
    ) -> NativeJSONValue {
        .object([
            "jsonrpc": .string("2.0"),
            "id": id ?? .null,
            "error": .object([
                "code": .number(Double(code)),
                "message": .string(message),
            ]),
        ])
    }
}

private struct NativeMCPRequest {
    var id: NativeJSONValue
    var method: String
    var name: String?
    var arguments: [String: NativeJSONValue]

    init(_ value: NativeJSONValue) throws {
        guard case let .object(root) = value,
              case let .string(method)? = root["method"] else {
            throw NativeMCPRelayError.invalidRequest
        }
        self.id = root["id"] ?? .null
        self.method = method
        let params: [String: NativeJSONValue]
        if case let .object(value)? = root["params"] { params = value }
        else { params = [:] }
        if case let .string(value)? = params["name"] { name = value }
        else { name = nil }
        if case let .object(arguments)? = params["arguments"] {
            self.arguments = arguments
        } else {
            self.arguments = [:]
        }
    }
}

private struct NativeMCPBuiltinPolicy {
    var codeRead = false
    var codeWrite = false
    var terminal = false
    var remoteConnection = false

    init(header: String?) {
        for token in (header ?? "").split(whereSeparator: { ",;| ".contains($0) }) {
            let normalized = token.lowercased().filter(\.isLetter)
            switch normalized {
            case "codemaintainerread": codeRead = true
            case "codemaintainerwrite":
                codeRead = true
                codeWrite = true
            case "terminalcontroller": terminal = true
            case "remoteconnectioncontroller": remoteConnection = true
            default: break
            }
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

    func number(_ key: String) -> Double? {
        guard case let .number(value)? = self[key] else { return nil }
        return value
    }
}

private enum NativeMCPRelayError: LocalizedError {
    case invalidRequest, invalidContext, emptyCommand

    var errorDescription: String? {
        switch self {
        case .invalidRequest: "MCP Relay 请求格式无效"
        case .invalidContext: "MCP Relay 请求与当前设备或工作区不匹配"
        case .emptyCommand: "MCP execute_command 缺少命令"
        }
    }
}
