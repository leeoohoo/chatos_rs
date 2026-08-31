import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    func handlePluginRelayMessage(
        _ data: Data,
        socket: URLSessionWebSocketTask
    ) async {
        let decoded = try? JSONDecoder().decode(NativeRelayRequest.self, from: data)
        let requestID = decoded?.requestID ?? ""
        let responseType = Self.pluginResponseType(decoded?.type)
        do {
            guard let request = decoded else {
                throw NativePluginRuntimeError.invalidRequest("Plugin Relay 请求格式无效")
            }
            let response = try await processPluginRelay(request)
            try await sendRelayResponse(response, socket: socket)
        } catch {
            try? await sendRelayResponse(
                .init(
                    type: responseType,
                    requestID: requestID,
                    status: 400,
                    body: .object(["error": .string(error.localizedDescription)])
                ),
                socket: socket
            )
        }
    }

    public func fetchPluginVisualSessions(
        loadFrameDataForAdapterSessionIDs: Set<String>? = nil
    ) async -> [PluginVisualSession] {
        let descriptors = await pluginRuntimeStore.visualDescriptors()
        return NativePluginVisualSessionReader.read(
            descriptors: descriptors,
            loadFrameDataForAdapterSessionIDs: loadFrameDataForAdapterSessionIDs
        )
    }

    private func processPluginRelay(_ request: NativeRelayRequest) async throws -> NativeRelayResponse {
        guard ["plugin_prepare_request", "plugin_execute_request", "plugin_cancel_request"]
            .contains(request.type),
              let ownerUserID = state.user?.id,
              let deviceID = state.deviceID else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Relay 与当前设备或工作区不匹配")
        }
        let scope = try NativePluginRelayScope.resolve(
            workspaceID: request.workspaceID,
            workspaces: state.workspaces
        )
        let token = try requireAccessToken()
        let runtime = try await gateway.managedRuntimeConfig(token: token)
        try NativeRelayVerifier().verify(
            request,
            trust: runtime.remoteControlTrust,
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            seenNonces: &seenRelayNonces
        )
        switch request.type {
        case "plugin_prepare_request":
            return try await preparePlugin(request, scope: scope)
        case "plugin_execute_request":
            return try await executePlugin(request, scope: scope)
        default:
            return try await cancelPlugin(request, scope: scope)
        }
    }

    private func preparePlugin(
        _ request: NativeRelayRequest,
        scope: NativePluginRelayScope
    ) async throws -> NativeRelayResponse {
        let body = try request.body.requireObject()
        let runID = try body.requireString("run_id")
        let pluginID = try body.requireString("plugin_id")
        let releaseID = try body.requireString("release_id")
        let artifactSHA256 = try body.requireString("artifact_sha256")
        let componentKey = try body.requireString("component_key")
        let serverKey = body["server_key"]?.jsonString
        let permissionSnapshot = Set(try body.requireStringArray("permission_snapshot"))
        let allowlist = Set(try body.optionalStringArray("tool_allowlist"))
        let blocklist = Set(try body.optionalStringArray("tool_blocklist"))
        try scope.validate(permissionSnapshot: permissionSnapshot)
        let projectRoot = try scope.projectRoot(for: request)
        guard state.pluginPreferences[pluginID] ?? true,
              let record = state.installedPluginRecords?[pluginID],
              record.releaseID == releaseID,
              record.artifactSHA256 == artifactSHA256.lowercased() else {
            throw NativePluginRuntimeError.invalidRequest("Plugin 未安装、已停用或 Release 不匹配")
        }
        let adapterSessionID = UUID().uuidString.lowercased()
        let skillKeys = try body.optionalStringArray("skill_keys")
        if !skillKeys.isEmpty {
            let prepared = try NativePluginSkillSnapshotLoader.prepareBody(
                record: record,
                componentKey: componentKey,
                skillKeys: skillKeys,
                expectedContentSHA256: body["content_sha256"]?.jsonString,
                runID: runID,
                adapterSessionID: adapterSessionID
            )
            return .init(
                type: "plugin_prepare_response",
                requestID: request.requestID,
                status: 200,
                body: prepared
            )
        }
        let launch = try NativePluginManifestLoader.prepare(
            record: record,
            componentKey: componentKey,
            serverKey: serverKey,
            adapterSessionID: adapterSessionID,
            workspaceRoot: projectRoot,
            permissionSnapshot: permissionSnapshot,
            runtimeRootURL: pluginRuntimeRootURL
        )
        let client = NativePluginStdioClient(launch: launch)
        do {
            try await client.start()
            let initialized = try await client.initialize()
            var seenNames = Set<String>()
            let tools = initialized.tools.filter { tool in
                guard let rawName = tool.jsonObject?["name"]?.jsonString else { return false }
                let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !name.isEmpty, seenNames.insert(name).inserted else { return false }
                return (allowlist.isEmpty || allowlist.contains(name)) && !blocklist.contains(name)
            }.sorted {
                ($0.jsonObject?["name"]?.jsonString ?? "")
                    < ($1.jsonObject?["name"]?.jsonString ?? "")
            }
            guard (1...200).contains(tools.count) else {
                throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP 工具数量无效")
            }
            let instructionsValue = initialized.instructions.map(NativeJSONValue.string) ?? .null
            let toolsValue = NativeJSONValue.array(tools)
            let toolHash = try NativePluginHash.canonicalSHA256(toolsValue)
            let instructionsHash = try NativePluginHash.canonicalSHA256(instructionsValue)
            let snapshotHash = try NativePluginHash.canonicalSHA256(.object([
                "identity": .string("\(pluginID):\(releaseID):\(launch.componentKey)"),
                "tools": .string(toolHash),
                "instructions": .string(instructionsHash),
            ]))
            let sessionHash = try NativePluginHash.canonicalSHA256(.object([
                "run_id": .string(runID),
                "adapter_session_id": .string(adapterSessionID),
                "snapshot_sha256": .string(snapshotHash),
            ]))
            let identity = NativePluginRuntimeStore.Identity(
                runID: runID,
                pluginID: pluginID,
                releaseID: releaseID,
                version: record.version,
                artifactSHA256: record.artifactSHA256,
                componentKey: launch.componentKey,
                adapterSessionID: adapterSessionID,
                requiresExclusiveExecution: launch.server.requiresExclusiveExecution
            )
            await pluginRuntimeStore.insert(
                identity: identity,
                client: client,
                tools: tools,
                permissionSnapshot: permissionSnapshot,
                displayName: launch.displayName,
                visualSessionURL: launch.visualSessionURL,
                artifactURL: launch.artifactURL,
                projectRootURL: projectRoot,
                workspaceID: scope.workspaceID
            )
            let expiresAt = Int(Date().addingTimeInterval(7 * 24 * 60 * 60).timeIntervalSince1970)
            return .init(
                type: "plugin_prepare_response",
                requestID: request.requestID,
                status: 200,
                body: .object([
                    "run_id": .string(runID),
                    "plugin_id": .string(pluginID),
                    "release_id": .string(releaseID),
                    "version": .string(record.version),
                    "artifact_sha256": .string(record.artifactSHA256),
                    "component_key": .string(launch.componentKey),
                    "mcp": .object([
                        "plugin_id": .string(pluginID),
                        "release_id": .string(releaseID),
                        "version": .string(record.version),
                        "artifact_sha256": .string(record.artifactSHA256),
                        "component_key": .string(launch.componentKey),
                        "oauth_connection_id": .null,
                        "server_instructions": instructionsValue,
                        "server_instructions_sha256": .string(instructionsHash),
                        "tools": toolsValue,
                        "tool_snapshot_sha256": .string(toolHash),
                        "snapshot_sha256": .string(snapshotHash),
                    ]),
                    "operations": .array([.string("mcp_tools_call"), .string("mcp_health_check")]),
                    "adapter_session_id": .string(adapterSessionID),
                    "session_sha256": .string(sessionHash),
                    "expires_at": .number(Double(expiresAt)),
                ])
            )
        } catch {
            await client.terminate()
            try? FileManager.default.removeItem(at: launch.visualSessionURL)
            throw error
        }
    }

    private func executePlugin(
        _ request: NativeRelayRequest,
        scope: NativePluginRelayScope
    ) async throws -> NativeRelayResponse {
        let body = try request.body.requireObject()
        let pluginID = try body.requireString("plugin_id")
        let releaseID = try body.requireString("release_id")
        let artifactSHA256 = try body.requireString("artifact_sha256")
        let componentKey = try body.requireString("component_key")
        let adapterSessionID = try body.requireString("adapter_session_id")
        let invocationID = try body.requireString("invocation_id")
        let operation = try body.requireString("operation")
        let toolName = try body.requireString("tool_name")
        guard operation == "mcp_tools_call" else {
            throw NativePluginRuntimeError.invalidRequest("不支持这个 Plugin 操作")
        }
        let identity = try await pluginRuntimeStore.validate(
            adapterSessionID: adapterSessionID,
            pluginID: pluginID,
            releaseID: releaseID,
            artifactSHA256: artifactSHA256,
            componentKey: componentKey,
            workspaceID: scope.workspaceID
        )
        if let conversationID = body["conversation_id"]?.jsonString?.nonEmptyTrimmed {
            await pluginRuntimeStore.bindOwner(
                .init(
                    conversationID: conversationID,
                    turnID: body["conversation_turn_id"]?.jsonString,
                    sourceUserMessageID: body["source_user_message_id"]?.jsonString,
                    taskID: body["task_id"]?.jsonString,
                    taskRunID: body["task_run_id"]?.jsonString,
                    taskTitle: body["task_title"]?.jsonString
                ),
                adapterSessionID: adapterSessionID
            )
        }
        let definition = await pluginRuntimeStore.toolDefinition(
            name: toolName,
            adapterSessionID: adapterSessionID
        )
        let policy = Self.toolPolicy(
            definition,
            componentKey: componentKey,
            toolName: toolName
        )
        let grantedPermissions = await pluginRuntimeStore.grantedPermissions(
            adapterSessionID: adapterSessionID
        )
        var toolArguments = body["arguments"] ?? .object([:])
        if toolName == "browser_session_open" {
            toolArguments = Self.browserSessionArguments(
                arguments: toolArguments,
                relayBody: body
            )
        }
        let requiredPermissions = policy.requiredPermissions(for: toolArguments)
        guard requiredPermissions.isSubset(of: grantedPermissions) else {
            throw NativePluginRuntimeError.permissionDenied("Plugin 工具请求了尚未授权的本机权限")
        }
        if policy.approvalMode == "per_call" {
            let projectRoot = try await pluginApprovalRoot(
                adapterSessionID: adapterSessionID
            )
            let operationSummary = Self.safeArgumentSummary(
                toolName: toolName,
                arguments: toolArguments
            )
            let approval = await approvalDecision(
                requestID: request.requestID,
                command: "\(Self.pluginSource(componentKey)) · \(toolName)",
                arguments: [operationSummary],
                cwd: projectRoot,
                projectRoot: projectRoot,
                source: Self.pluginSource(componentKey),
                risk: .init(
                    level: policy.riskLevel,
                    reason: "Plugin 请求执行本机操作：\(operationSummary)"
                ),
                requestedPermissionsDescription: Self.permissionDescription(
                    toolName: toolName,
                    requiredPermissions: requiredPermissions
                ),
                approvalScopeKey: "plugin:\(adapterSessionID)"
            )
            guard case .approve = approval else {
                throw NativePluginRuntimeError.permissionDenied("用户未批准这次 Plugin 操作")
            }
        }
        let rawResult = try await pluginRuntimeStore.call(
            adapterSessionID: adapterSessionID,
            invocationID: invocationID,
            toolName: toolName,
            arguments: toolArguments,
            timeout: .milliseconds(policy.timeoutMilliseconds)
        )
        guard let ownerUserID = state.user?.id, let deviceID = state.deviceID else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Relay 的设备身份已失效")
        }
        let registeredResult = try await pluginRuntimeStore.registerArtifacts(
            adapterSessionID: adapterSessionID,
            result: rawResult,
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            workspaceID: scope.workspaceID,
            toolName: toolName
        )
        let result = NativePluginModelImageNormalizer.normalizeForModel(registeredResult)
        return .init(
            type: "plugin_execute_response",
            requestID: request.requestID,
            status: 200,
            body: .object([
                "plugin_id": .string(identity.pluginID),
                "release_id": .string(identity.releaseID),
                "version": .string(identity.version),
                "artifact_sha256": .string(identity.artifactSHA256),
                "component_key": .string(identity.componentKey),
                "invocation_id": .string(invocationID),
                "tool_name": .string(toolName),
                "adapter_session_id": .string(adapterSessionID),
                "operation": .string(operation),
                "result": result,
            ])
        )
    }

    private func cancelPlugin(
        _ request: NativeRelayRequest,
        scope: NativePluginRelayScope
    ) async throws -> NativeRelayResponse {
        let body = try request.body.requireObject()
        let runID = try body.requireString("run_id")
        let adapterSessionID = try body.requireString("adapter_session_id")
        let invocationID = body["invocation_id"]?.jsonString?.nonEmptyTrimmed
        try await pluginRuntimeStore.validateScopeIfPresent(
            adapterSessionID: adapterSessionID,
            workspaceID: scope.workspaceID
        )
        let status = await pluginRuntimeStore.cancel(
            adapterSessionID: adapterSessionID,
            invocationID: invocationID
        )
        return .init(
            type: "plugin_cancel_response",
            requestID: request.requestID,
            status: 200,
            body: .object([
                "run_id": .string(runID),
                "adapter_session_id": .string(adapterSessionID),
                "invocation_id": .string(invocationID ?? ""),
                "status": .string(status),
            ])
        )
    }

    private func pluginApprovalRoot(adapterSessionID: String) async throws -> URL {
        if let projectRoot = await pluginRuntimeStore.projectRootURL(
            adapterSessionID: adapterSessionID
        ) {
            return projectRoot
        }
        let root = pluginRuntimeRootURL
            .appendingPathComponent("device-only-approval", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    private static func pluginResponseType(_ requestType: String?) -> String {
        switch requestType {
        case "plugin_execute_request": "plugin_execute_response"
        case "plugin_cancel_request": "plugin_cancel_response"
        default: "plugin_prepare_response"
        }
    }

    private static func pluginSource(_ componentKey: String) -> String {
        componentKey.localizedCaseInsensitiveContains("browser")
            ? "plugin_browser_cdp"
            : "plugin_computer_use"
    }

    static func safeArgumentSummary(
        toolName: String,
        arguments: NativeJSONValue
    ) -> String {
        if toolName == "browser_session_open" {
            let object = arguments.jsonObject ?? [:]
            let mode = object["mode"]?.jsonString ?? "managed"
            let headless = object["headless"]?.jsonBool ?? true
            let persistentProfile = object["persistent_profile"]?.jsonBool ?? false
            let sessionName = object["session_name"]?.jsonString ?? "ChatOS Browser"
            return "启动隔离浏览器会话：任务 \(sessionName)，模式 \(mode)，Headless \(headless ? "是" : "否")，持久化浏览器资料 \(persistentProfile ? "是" : "否")。"
        }
        if toolName == "browser_cdp_attach" {
            return "为当前隔离浏览器中的指定标签页建立临时 CDP 会话；浏览器会话 ID 与标签页 ID 均为 ChatOS 生成的不透明标识。"
        }
        if toolName == "browser_cdp_detach" {
            return "结束当前隔离浏览器中的临时 CDP 会话。"
        }
        if toolName == "browser_cdp_send" {
            let object = arguments.jsonObject ?? [:]
            let method = object["method"]?.jsonString ?? "未知方法"
            let target = object["target"]?.jsonString ?? "page"
            let parameterKeys = object["params"]?.jsonObject?.keys.sorted().joined(separator: ", ")
                ?? "无"
            let expression = object["params"]?.jsonObject?["expression"]?.jsonString
            let expressionSummary = safeCDPExpressionSummary(expression)
            return "向当前隔离浏览器发送 CDP 方法 \(method)，目标 \(target)，参数字段：\(parameterKeys)\(expressionSummary.map { "；表达式：\($0)" } ?? "")。"
        }
        let keys = arguments.jsonObject?.keys.sorted().joined(separator: ", ") ?? "参数"
        let digest = (try? NativePluginHash.canonicalSHA256(arguments).prefix(12)) ?? "unknown"
        return "字段：\(keys)；内容摘要：\(digest)"
    }

    static func browserSessionArguments(
        arguments: NativeJSONValue,
        relayBody: [String: NativeJSONValue]
    ) -> NativeJSONValue {
        guard var object = arguments.jsonObject else { return arguments }
        if object["session_name"]?.jsonString?.nonEmptyTrimmed != nil {
            return arguments
        }
        let title = relayBody["task_title"]?.jsonString?.nonEmptyTrimmed
            ?? relayBody["task_id"]?.jsonString?.nonEmptyTrimmed.map {
                "ChatOS · \(String($0.prefix(12)))"
            }
            ?? relayBody["task_run_id"]?.jsonString?.nonEmptyTrimmed.map {
                "ChatOS · \(String($0.prefix(12)))"
            }
            ?? "ChatOS Browser"
        object["session_name"] = .string(String(title.prefix(80)))
        return .object(object)
    }

    static func permissionDescription(
        toolName: String,
        requiredPermissions: Set<String>
    ) -> String {
        let permissions = requiredPermissions.sorted().joined(separator: ", ")
        if toolName == "browser_session_open" {
            return "启动由 ChatOS 管理的隔离 Chrome 会话；不连接用户现有 Chrome，也不复用用户浏览器资料。所需权限：\(permissions)"
        }
        if toolName.hasPrefix("browser_cdp_") {
            return "仅操作当前 ChatOS 隔离浏览器会话中的临时 CDP 连接。所需权限：\(permissions)"
        }
        return permissions.isEmpty ? "未声明额外权限。" : "所需权限：\(permissions)"
    }

    private static func safeCDPExpressionSummary(_ expression: String?) -> String? {
        guard let expression = expression?.trimmingCharacters(in: .whitespacesAndNewlines),
              !expression.isEmpty else { return nil }
        let sensitiveMarkers = [
            "authorization", "bearer", "password", "passwd", "secret", "token",
            "document.cookie", "localstorage", "sessionstorage",
        ]
        let normalized = expression.lowercased()
        guard !sensitiveMarkers.contains(where: normalized.contains) else {
            return "已隐藏敏感表达式"
        }
        let compact = expression.replacingOccurrences(of: "\n", with: " ")
        return String(compact.prefix(240))
    }

    private static func toolPolicy(
        _ tool: NativeJSONValue?,
        componentKey: String,
        toolName: String
    ) -> NativePluginToolPolicy {
        let meta = tool?.jsonObject?["_meta"]?.jsonObject
        let approval = meta?["chatos/approvalMode"]?.jsonString ?? "none"
        let risk = meta?["chatos/riskLevel"]?.jsonString ?? "low"
        let declaredTimeout = Int(
            meta?["chatos/timeoutMs"]?.jsonNumber
                ?? defaultPluginToolTimeoutMilliseconds(
                    componentKey: componentKey,
                    toolName: toolName
                )
        )
        let required = Set(
            meta?["chatos/requiredPermissions"]?.jsonArray?.compactMap(\.jsonString) ?? []
        )
        let rules: [NativePluginPermissionRule] = meta?["chatos/permissionRules"]?.jsonArray?.compactMap { value in
            guard let object = value.jsonObject,
                  let pointer = object["argumentPointer"]?.jsonString,
                  let permissions = object["requiredPermissions"]?.jsonArray else { return nil }
            return NativePluginPermissionRule(
                argumentPointer: pointer,
                expectedValue: object["equals"] ?? .null,
                matchWhenMissing: object["matchWhenMissing"]?.jsonBool ?? false,
                requiredPermissions: Set(permissions.compactMap(\.jsonString))
            )
        } ?? []
        return NativePluginToolPolicy(
            approvalMode: approval == "per_call" ? "per_call" : "none",
            riskLevel: ["low", "medium", "high", "critical"].contains(risk) ? risk : "low",
            timeoutMilliseconds: pluginToolHostTimeoutMilliseconds(
                declaredTimeoutMilliseconds: declaredTimeout
            ),
            requiredPermissions: required,
            permissionRules: rules
        )
    }

    static func defaultPluginToolTimeoutMilliseconds(
        componentKey: String,
        toolName: String
    ) -> Double {
        _ = componentKey
        _ = toolName
        // The task execution contract allows a tool call to run for up to two
        // hours. Individual transports and operations still own shorter
        // watchdogs and must return immediately when they observe a concrete
        // failure. This outer deadline is only the final safety ceiling; it
        // must not terminate a legitimate long-running tool operation early.
        return 7_200_000
    }

    static func pluginToolHostTimeoutMilliseconds(
        declaredTimeoutMilliseconds: Int
    ) -> Int {
        let bounded = min(7_200_000, max(300, declaredTimeoutMilliseconds))
        guard bounded < 7_200_000 else { return bounded }
        // Plugin tool metadata commonly describes the operation's own timeout.
        // Leave enough time for the plugin to serialize and return its normal
        // timeout error instead of racing the host watchdog and losing the
        // entire stdio MCP process at the exact same deadline.
        let grace = min(10_000, max(2_000, bounded / 2))
        return min(7_200_000, bounded + grace)
    }
}

private struct NativePluginToolPolicy {
    var approvalMode: String
    var riskLevel: String
    var timeoutMilliseconds: Int
    var requiredPermissions: Set<String>
    var permissionRules: [NativePluginPermissionRule]

    func requiredPermissions(for arguments: NativeJSONValue) -> Set<String> {
        permissionRules.reduce(into: requiredPermissions) { result, rule in
            let value = arguments.value(atJSONPointer: rule.argumentPointer)
            if value == rule.expectedValue || (value == nil && rule.matchWhenMissing) {
                result.formUnion(rule.requiredPermissions)
            }
        }
    }
}

private struct NativePluginPermissionRule {
    var argumentPointer: String
    var expectedValue: NativeJSONValue
    var matchWhenMissing: Bool
    var requiredPermissions: Set<String>
}

extension NativeJSONValue {
    func requireObject() throws -> [String: NativeJSONValue] {
        guard let jsonObject else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Relay body 必须是对象")
        }
        return jsonObject
    }

    func value(atJSONPointer pointer: String) -> NativeJSONValue? {
        guard pointer.hasPrefix("/") else { return nil }
        return pointer.dropFirst().split(separator: "/").reduce(Optional(self)) { current, token in
            guard let current else { return nil }
            let key = token.replacingOccurrences(of: "~1", with: "/")
                .replacingOccurrences(of: "~0", with: "~")
            return current.jsonObject?[key]
        }
    }
}

extension Dictionary where Key == String, Value == NativeJSONValue {
    func requireString(_ key: String) throws -> String {
        guard let value = self[key]?.jsonString?.nonEmptyTrimmed else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Relay 缺少 \(key)")
        }
        return value
    }

    func requireStringArray(_ key: String) throws -> [String] {
        guard let values = self[key]?.jsonArray else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Relay 缺少 \(key)")
        }
        return try values.map {
            guard let value = $0.jsonString?.nonEmptyTrimmed else {
                throw NativePluginRuntimeError.invalidRequest("Plugin Relay 的 \(key) 无效")
            }
            return value
        }
    }

    func optionalStringArray(_ key: String) throws -> [String] {
        guard self[key] != nil else { return [] }
        return try requireStringArray(key)
    }
}

private extension String {
    var nonEmptyTrimmed: String? {
        let result = trimmingCharacters(in: .whitespacesAndNewlines)
        return result.isEmpty ? nil : result
    }
}
