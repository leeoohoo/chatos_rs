import ChatOSAgentRuntime
import ChatOSCore
import Foundation

struct NativeAgentBuiltinToolProvider: AgentToolProvider, Sendable {
    private let service: NativeLocalConnectorService
    private let runContext: LocalAgentChatRunContext
    private let resolvedProject: NativeResolvedProjectPath
    private let allowedCapabilities: Set<LocalAgentTodoBuiltinCapability>
    private let lease: NativeAgentBuiltinRunLease

    init(
        service: NativeLocalConnectorService,
        runContext: LocalAgentChatRunContext,
        resolvedProject: NativeResolvedProjectPath,
        allowedCapabilities: Set<LocalAgentTodoBuiltinCapability>
    ) {
        self.service = service
        self.runContext = runContext
        self.resolvedProject = resolvedProject
        self.allowedCapabilities = allowedCapabilities
        self.lease = .init(service: service, runID: runContext.runID)
    }

    func definitions() async throws -> [AgentToolDefinition] {
        _ = lease
        return try Self.nativeDefinitions.filter { value in
            guard let name = value.jsonObject?["name"]?.jsonString else { return false }
            return allowedCapabilities.contains(Self.capability(for: name))
        }.map { value in
            guard let object = value.jsonObject,
                  let name = object["name"]?.jsonString,
                  let description = object["description"]?.jsonString,
                  let schema = object["inputSchema"] else {
                throw NativePluginRuntimeError.invalidMCPResponse("内置 MCP 工具定义无效")
            }
            let effect: AgentToolDefinition.Effect
            if NativeMCPCodeWriteStore.toolNames.contains(name)
                || NativeMCPRequirementSurveyTools.writeToolNames.contains(name) {
                effect = .write
            } else if NativeMCPTerminalStore.toolNames.contains(name) {
                effect = .terminal
            } else {
                effect = .readOnly
            }
            return .init(
                name: name,
                description: description,
                schema: try JSONEncoder().encode(schema),
                effect: effect,
                providerID: Self.providerID(for: name),
                skillBindingID: Self.skillBindingID(for: name)
            )
        }
    }

    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        do {
            try Task.checkCancellation()
            guard allowedCapabilities.contains(Self.capability(for: call.name)) else {
                return .failure("这个 Todo 的可信执行计划没有授权该基础能力。")
            }
            let value = try JSONDecoder().decode(
                NativeJSONValue.self,
                from: Data(call.arguments.utf8)
            )
            guard case let .object(arguments) = value else {
                return .failure("内置 MCP 工具参数必须是 JSON 对象。")
            }
            let result = try await service.executeAgentBuiltinTool(
                callID: call.id,
                name: call.name,
                arguments: arguments,
                runContext: runContext,
                resolvedProject: resolvedProject
            )
            try Task.checkCancellation()
            let content = result.canonicalJSONString
            if call.name == "execute_command",
               let values = result.jsonObject,
               values["timed_out"]?.jsonBool == true
                || values["success"]?.jsonBool == false {
                return .failure(content)
            }
            return .init(content)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            return .failure(error.localizedDescription)
        }
    }

    private static let nativeDefinitions = NativeMCPCodeReadTools.toolDefinitions
        + NativeMCPCodeWriteStore.toolDefinitions
        + NativeMCPTerminalStore.toolDefinitions
        + NativeMCPRequirementSurveyTools.readToolDefinitions
        + NativeMCPRequirementSurveyTools.writeToolDefinitions

    /// Enumerates every native model-visible builtin through the same metadata path used by the
    /// live provider. Migration remains audit-only until the report is complete for all families.
    static func skillCoverageReport() throws -> ToolSkillCoverageReport {
        let tools = try nativeDefinitions.map { value -> ToolSkillCoverageInput in
            guard let name = value.jsonObject?["name"]?.jsonString else {
                throw NativePluginRuntimeError.invalidMCPResponse("内置 MCP 工具定义缺少名称")
            }
            return .init(
                providerID: providerID(for: name),
                toolName: name,
                skillBindingID: skillBindingID(for: name)
            )
        }
        return ToolSkillCoverageCatalog.product.audit(tools)
    }

    private static func providerID(for toolName: String) -> String {
        if NativeMCPCodeWriteStore.toolNames.contains(toolName) {
            return ProductToolProviderID.projectWrite
        }
        if NativeMCPTerminalStore.toolNames.contains(toolName) {
            return ProductToolProviderID.terminal
        }
        if NativeMCPRequirementSurveyTools.readToolNames.contains(toolName)
            || NativeMCPRequirementSurveyTools.writeToolNames.contains(toolName) {
            return ProductToolProviderID.requirementSurvey
        }
        return ProductToolProviderID.projectRead
    }

    private static func skillBindingID(for toolName: String) -> String? {
        switch toolName {
        case "execute_command":
            ProductToolSkillBindingID.terminalCommandExecution
        case "get_recent_logs", "process_list", "process_poll", "process_log", "process_wait":
            ProductToolSkillBindingID.terminalProcessObservation
        case "process_write", "process_kill", "process":
            ProductToolSkillBindingID.terminalProcessControl
        default:
            nil
        }
    }

    private static func capability(for toolName: String) -> LocalAgentTodoBuiltinCapability {
        if NativeMCPCodeWriteStore.toolNames.contains(toolName) { return .projectWrite }
        if NativeMCPTerminalStore.toolNames.contains(toolName) { return .terminal }
        if NativeMCPRequirementSurveyTools.writeToolNames.contains(toolName) {
            return .requirementSurveyWrite
        }
        if NativeMCPRequirementSurveyTools.readToolNames.contains(toolName) {
            return .requirementSurveyRead
        }
        return .projectRead
    }
}

extension NativeLocalConnectorService {
    func executeAgentBuiltinTool(
        callID: String,
        name: String,
        arguments: [String: NativeJSONValue],
        runContext: LocalAgentChatRunContext,
        resolvedProject: NativeResolvedProjectPath
    ) async throws -> NativeJSONValue {
        guard runContext.ownerUserID == state.user?.id,
              !runContext.projectID.hasPrefix("direct:") else {
            throw NativePluginRuntimeError.invalidRequest("当前 Agent 会话没有绑定项目")
        }
        let projectRoot = resolvedProject.absoluteURL
        if NativeMCPRequirementSurveyTools.readToolNames.contains(name)
            || NativeMCPRequirementSurveyTools.writeToolNames.contains(name) {
            guard let agentGroupChatService else {
                throw NativePluginRuntimeError.invalidRequest("需求调研存储尚未连接")
            }
            let store = try await agentGroupChatService.store()
            return try await NativeMCPRequirementSurveyTools(
                store: store,
                ownerUserID: runContext.ownerUserID,
                projectID: runContext.projectID,
                creatorAgentID: runContext.agentID,
                sourceDeliveryID: runContext.deliveryID,
                now: { Int64(Date().timeIntervalSince1970 * 1_000) }
            ).call(name: name, arguments: arguments)
        }
        if NativeMCPCodeReadTools.toolDefinitions.contains(where: {
            $0.jsonObject?["name"]?.jsonString == name
        }) {
            let readTask = Task.detached {
                try NativeMCPCodeReadTools(
                    workspace: resolvedProject.workspace,
                    projectRoot: projectRoot,
                    requestCWD: nil,
                    defaultToolRoot: nil
                ).call(name: name, arguments: arguments)
            }
            let result = try await withTaskCancellationHandler {
                try await readTask.value
            } onCancel: {
                readTask.cancel()
            }
            try await appendAgentBuiltinAudit(
                name: name,
                arguments: arguments,
                result: result,
                runContext: runContext
            )
            return result
        }
        if NativeMCPCodeWriteStore.toolNames.contains(name) {
            // Reaching this branch already proves that this is a Todo executor whose immutable
            // execution plan selected `project_write`: the capability broker is only assembled
            // for executor runs and NativeAgentBuiltinToolProvider checks the selected capability
            // again before dispatch. Asking the global command-approval system here would be a
            // second, unrelated authorization that can strand an otherwise approved Todo at the
            // final commit step.
            try Task.checkCancellation()
            let result = try await mcpCodeWriteStore.call(
                name: name,
                arguments: arguments,
                scope: .init(
                    workspaceID: resolvedProject.workspace.id,
                    sessionID: runContext.roomID,
                    runID: runContext.runID
                ),
                projectRoot: projectRoot
            )
            try await appendAgentBuiltinAudit(
                name: name,
                arguments: arguments,
                result: result,
                runContext: runContext
            )
            return result
        }
        guard NativeMCPTerminalStore.toolNames.contains(name) else {
            throw NativePluginRuntimeError.invalidRequest("当前项目没有这个内置 MCP 工具")
        }
        if name != "execute_command" {
            return try await mcpTerminalStore.call(
                name: name,
                arguments: arguments,
                projectRoot: projectRoot
            )
        }
        let command = arguments["common"]?.jsonString
            ?? arguments["command"]?.jsonString
            ?? ""
        guard !command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw NativePluginRuntimeError.invalidRequest("终端命令不能为空")
        }
        let cwd = try resolveDirectory(
            arguments["path"]?.jsonString ?? ".",
            relativeTo: projectRoot,
            workspace: resolvedProject.workspace
        )
        let shellArguments = ["-lc", command]
        let risk = NativeApprovalRiskEvaluator.evaluate(
            command: "/bin/zsh",
            arguments: shellArguments
        )
        let decision = await approvalDecision(
            requestID: callID,
            command: "/bin/zsh",
            arguments: shellArguments,
            cwd: cwd,
            projectRoot: projectRoot,
            source: "local-agent-builtin-mcp",
            risk: risk,
            approvalScopeKey: "agent-project-terminal",
            workspaceID: resolvedProject.workspace.id
        )
        guard case .approve = decision else {
            throw NativePluginRuntimeError.invalidRequest("用户未批准 Agent 执行终端命令")
        }
        try Task.checkCancellation()
        return try await mcpTerminalStore.execute(
            command: command,
            cwd: cwd,
            projectRoot: projectRoot,
            background: arguments["background"]?.jsonBool ?? false,
            timeoutMilliseconds: arguments["timeout_ms"]?.jsonNumber.map { Int($0) }
                ?? arguments["timeout"]?.jsonNumber.map { Int($0 * 1_000) },
            ownerRunID: runContext.runID
        )
    }

    private func appendAgentBuiltinAudit(
        name: String,
        arguments: [String: NativeJSONValue],
        result: NativeJSONValue,
        runContext: LocalAgentChatRunContext
    ) async throws {
        guard runContext.lane == .executor, let agentGroupChatService else { return }
        let store = try await agentGroupChatService.store()
        guard let todo = try await store.todoForDelivery(
            ownerUserID: runContext.ownerUserID,
            deliveryID: runContext.deliveryID
        ), todo.agentID == runContext.agentID else { return }

        let audit: (stage: String, detail: String)?
        let object = result.jsonObject ?? [:]
        let resultObject = object["result"]?.jsonObject ?? [:]
        switch name {
        case "read_file_raw", "read_file_range", "read_file":
            let path = object["path"]?.jsonString ?? arguments["path"]?.jsonString ?? "(unknown)"
            let hash = object["sha256"]?.jsonString.map { "，sha256=\($0)" } ?? ""
            audit = ("builtin.file_read", "已通过 \(name) 读取项目文件：\(path)\(hash)")
        case "list_dir":
            let path = arguments["path"]?.jsonString ?? "."
            audit = ("builtin.directory_list", "已通过 list_dir 列出项目目录：\(path)")
        case "search_text", "search_files":
            let path = arguments["path"]?.jsonString ?? "."
            audit = ("builtin.project_search", "已通过 \(name) 搜索项目范围：\(path)")
        case "open_edit_session":
            let sessionID = resultObject["session_id"]?.jsonString ?? "(unknown)"
            audit = ("builtin.edit_opened", "已打开事务编辑会话：\(sessionID)")
        case "stage_edit_batch":
            let paths = resultObject["batch_changed_paths"]?.jsonArray?
                .compactMap(\.jsonString) ?? []
            audit = (
                "builtin.edit_staged",
                "已暂存项目文件修改：\(paths.isEmpty ? "无实际变化" : paths.joined(separator: ", "))"
            )
        case "commit_edit_session":
            let paths = resultObject["committed_paths"]?.jsonArray?
                .compactMap(\.jsonString) ?? []
            audit = (
                "builtin.edit_committed",
                "已提交事务编辑：\(paths.isEmpty ? "无实际变化" : paths.joined(separator: ", "))"
            )
        case "abort_edit_session":
            audit = ("builtin.edit_aborted", "已放弃当前事务编辑会话。")
        default:
            audit = nil
        }
        guard let audit else { return }
        _ = try await store.appendAgentTodoProgress(
            ownerUserID: runContext.ownerUserID,
            agentID: runContext.agentID,
            todoID: todo.id,
            kind: .progress,
            runID: runContext.runID,
            stage: audit.stage,
            detail: String(audit.detail.prefix(16_000)),
            assetUpdateSuggestions: [],
            nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
        )
    }

    func cancelAgentBuiltinTools(runID: String) async {
        _ = await mcpTerminalStore.cancel(ownerRunID: runID)
    }

    func discardAgentBuiltinEdits(runID: String) async {
        _ = await mcpCodeWriteStore.discard(runID: runID)
    }
}

private final class NativeAgentBuiltinRunLease: @unchecked Sendable {
    private let service: NativeLocalConnectorService
    private let runID: String

    init(service: NativeLocalConnectorService, runID: String) {
        self.service = service
        self.runID = runID
    }

    deinit {
        let service = service
        let runID = runID
        // A provider lease ends after every scheduler attempt, including a resumable model
        // timeout. Stop live terminal processes, but keep the run-scoped transactional edit
        // session so the same run can resume, inspect, commit, or explicitly abort it.
        Task { await service.cancelAgentBuiltinTools(runID: runID) }
    }
}

struct NativeAgentPluginToolProvider: AgentToolProvider, Sendable {
    private let service: NativeLocalConnectorService
    private let runtimeStore: NativePluginRuntimeStore
    private let identity: NativePluginRuntimeStore.Identity
    private let nativeToolsByName: [String: NativeJSONValue]
    private let agentDefinitions: [AgentToolDefinition]
    private let ownerUserID: String
    private let projectRootURL: URL
    private let workspaceID: String
    private let permissionSnapshot: Set<String>
    private let lease: NativeAgentPluginSessionLease

    init(
        service: NativeLocalConnectorService,
        runtimeStore: NativePluginRuntimeStore,
        identity: NativePluginRuntimeStore.Identity,
        tools: [NativeJSONValue],
        displayName: String,
        ownerUserID: String,
        projectRootURL: URL,
        workspaceID: String,
        permissionSnapshot: Set<String>
    ) throws {
        self.service = service
        self.runtimeStore = runtimeStore
        self.identity = identity
        self.ownerUserID = ownerUserID
        self.projectRootURL = projectRootURL
        self.workspaceID = workspaceID
        self.permissionSnapshot = permissionSnapshot
        self.lease = .init(runtimeStore: runtimeStore, adapterSessionID: identity.adapterSessionID)

        var nativeToolsByName: [String: NativeJSONValue] = [:]
        var definitions: [AgentToolDefinition] = []
        for tool in tools {
            guard let object = tool.jsonObject,
                  let name = object["name"]?.jsonString,
                  let schemaValue = object["inputSchema"] else {
                throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP 工具定义无效")
            }
            let schema = try JSONEncoder().encode(schemaValue)
            guard schema.count <= 512 * 1_024 else {
                throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP 工具 Schema 过大")
            }
            let description = object["description"]?.jsonString?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            nativeToolsByName[name] = tool
            definitions.append(.init(
                name: name,
                description: "[\(displayName)] " + (description?.isEmpty == false
                    ? description!
                    : "本地已安装 Plugin 提供的工具"),
                schema: schema,
                effect: Self.effect(for: tool)
            ))
        }
        self.nativeToolsByName = nativeToolsByName
        self.agentDefinitions = definitions
    }

    func definitions() async throws -> [AgentToolDefinition] {
        _ = lease
        return agentDefinitions
    }

    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        try Task.checkCancellation()
        guard let definition = nativeToolsByName[call.name],
              let data = call.arguments.data(using: .utf8) else {
            return .failure("Plugin 工具不可用：\(call.name)")
        }
        var arguments = try JSONDecoder().decode(NativeJSONValue.self, from: data)
        guard arguments.jsonObject != nil else {
            return .failure("Plugin 工具参数必须是 JSON 对象。")
        }
        let policy = NativeLocalConnectorService.toolPolicy(
            definition,
            componentKey: identity.componentKey,
            toolName: call.name
        )
        if call.name == "browser_session_open" {
            let paired = (try? await service.isBrowserExtensionPaired(pluginID: identity.pluginID)) == true
            arguments = NativeLocalConnectorService.browserSessionArguments(
                arguments: arguments,
                relayBody: [:],
                browserExtensionPaired: paired
            )
        }
        let requiredPermissions = policy.requiredPermissions(for: arguments)
        guard requiredPermissions.isSubset(of: permissionSnapshot) else {
            return .failure("Plugin 工具请求了尚未授权的本机权限。")
        }
        guard await service.approveAgentPluginTool(
            callID: call.id,
            componentKey: identity.componentKey,
            toolName: call.name,
            arguments: arguments,
            policy: policy,
            projectRootURL: projectRootURL,
            workspaceID: workspaceID
        ) else {
            return .failure("用户未批准这次 Plugin 操作。")
        }
        try Task.checkCancellation()
        let rawResult = try await runtimeStore.call(
            adapterSessionID: identity.adapterSessionID,
            invocationID: call.id,
            toolName: call.name,
            arguments: arguments,
            timeout: .milliseconds(policy.timeoutMilliseconds)
        )
        try Task.checkCancellation()
        let deviceID = try await service.localProjectDeviceID(ownerUserID: ownerUserID)
        guard let deviceID else { return .failure("Plugin 本机设备身份无效。") }
        let registered = try await runtimeStore.registerArtifacts(
            adapterSessionID: identity.adapterSessionID,
            result: rawResult,
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            workspaceID: workspaceID,
            toolName: call.name
        )
        let normalized = NativePluginModelImageNormalizer.normalizeForModel(registered)
        return .init(normalized.canonicalJSONString)
    }

    private static func effect(for tool: NativeJSONValue) -> AgentToolDefinition.Effect {
        let object = tool.jsonObject ?? [:]
        let meta = object["_meta"]?.jsonObject ?? [:]
        switch meta["chatos/effect"]?.jsonString {
        case "read_only", "readOnly": return .readOnly
        case "billable": return .billable
        case "write": return .write
        default:
            if meta["chatos/billable"]?.jsonBool == true { return .billable }
            if object["annotations"]?.jsonObject?["readOnlyHint"]?.jsonBool == true {
                return .readOnly
            }
            return .write
        }
    }
}

private final class NativeAgentPluginSessionLease: @unchecked Sendable {
    private let runtimeStore: NativePluginRuntimeStore
    private let adapterSessionID: String

    init(runtimeStore: NativePluginRuntimeStore, adapterSessionID: String) {
        self.runtimeStore = runtimeStore
        self.adapterSessionID = adapterSessionID
    }

    deinit {
        let runtimeStore = runtimeStore
        let adapterSessionID = adapterSessionID
        Task {
            _ = await runtimeStore.cancel(
                adapterSessionID: adapterSessionID,
                invocationID: nil
            )
        }
    }
}
