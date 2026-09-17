import ChatOSAgentRuntime
import ChatOSCore
import Foundation

public struct NativeInstalledAgentPlugin: Sendable, Equatable, Identifiable {
    public let id: String
    public let displayName: String
    public let description: String
    public let componentCount: Int

    public init(id: String, displayName: String, description: String, componentCount: Int) {
        self.id = id
        self.displayName = displayName
        self.description = description
        self.componentCount = componentCount
    }
}

/// Starts already-installed stdio MCP plugins directly from the client for one local Agent run.
/// It deliberately bypasses relay request signing because there is no remote caller: account,
/// project, Agent and plugin selection have already been fixed by local application state.
extension NativeLocalConnectorService {
    public func installedAgentPlugins(ownerUserID: String) throws -> [NativeInstalledAgentPlugin] {
        guard state.user?.id == ownerUserID else {
            throw NativePluginRuntimeError.invalidRequest("本机 Plugin 不属于当前账户")
        }
        return (state.installedPluginRecords ?? [:]).values.compactMap { record in
            guard state.pluginPreferences[record.pluginID] ?? true,
                  let manifest = try? Self.agentPluginManifest(record: record) else { return nil }
            return .init(
                id: record.pluginID,
                displayName: manifest.interface?.displayName ?? manifest.name,
                description: manifest.description,
                componentCount: manifest.mcpServers.count
            )
        }.sorted {
            if $0.displayName != $1.displayName {
                return $0.displayName.localizedStandardCompare($1.displayName) == .orderedAscending
            }
            return $0.id < $1.id
        }
    }

    public func makeAgentPluginToolProviders(
        ownerUserID: String,
        runContext: LocalAgentChatRunContext,
        pluginIDs: [String],
        projectContext: LocalConnectorPluginApplicationContext
    ) async throws -> [any AgentToolProvider] {
        guard state.user?.id == ownerUserID,
              runContext.ownerUserID == ownerUserID,
              projectContext.projectID == runContext.projectID,
              let deviceID = state.deviceID,
              let rawProjectRoot = projectContext.projectRoot else {
            throw NativePluginRuntimeError.invalidRequest("本地 Agent Plugin 与当前账户或项目不匹配")
        }
        let resolvedProject = try resolveProjectPath(rawProjectRoot)
        let selectedPluginIDs = Array(Set(pluginIDs)).sorted()
        guard selectedPluginIDs.count <= 100 else {
            throw NativePluginRuntimeError.invalidRequest("本地 Agent 选择的 Plugin 数量无效")
        }

        var providers: [any AgentToolProvider] = []
        for pluginID in selectedPluginIDs {
            guard state.pluginPreferences[pluginID] ?? true,
                  let record = state.installedPluginRecords?[pluginID] else {
                throw NativePluginRuntimeError.invalidRequest("Agent 配置的 Plugin 未安装或已停用：\(pluginID)")
            }
            let manifest = try Self.agentPluginManifest(record: record)
            let permissionSnapshot = Set(manifest.permissions.map(\.permission))
            for componentKey in manifest.mcpServers.keys.sorted() {
                let adapterSessionID = "agent-" + UUID().uuidString.lowercased()
                if componentKey == NativeBrowserPluginIdentity.componentKey {
                    await browserExtensionPairingRuntime.stop()
                }
                let launch = try NativePluginManifestLoader.prepare(
                    record: record,
                    componentKey: componentKey,
                    serverKey: componentKey,
                    adapterSessionID: adapterSessionID,
                    ownerUserID: ownerUserID,
                    deviceID: deviceID,
                    workspaceID: resolvedProject.workspace.id,
                    workspaceRoot: resolvedProject.absoluteURL,
                    projectID: projectContext.projectID,
                    projectName: projectContext.projectName,
                    permissionSnapshot: permissionSnapshot,
                    runtimeRootURL: pluginRuntimeRootURL
                )
                let client = NativePluginStdioClient(launch: launch)
                do {
                    try await client.start()
                    let initialized = try await client.initialize()
                    let tools = try Self.validatedAgentPluginTools(initialized.tools)
                    let identity = NativePluginRuntimeStore.Identity(
                        runID: runContext.runID,
                        pluginID: pluginID,
                        releaseID: record.releaseID,
                        version: record.version,
                        artifactSHA256: record.artifactSHA256,
                        componentKey: componentKey,
                        adapterSessionID: adapterSessionID,
                        projectID: runContext.projectID,
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
                        projectRootURL: resolvedProject.absoluteURL,
                        workspaceID: resolvedProject.workspace.id
                    )
                    await pluginRuntimeStore.bindOwner(
                        .init(
                            conversationID: runContext.roomID,
                            sourceUserMessageID: runContext.triggerMessageID,
                            taskID: runContext.deliveryID,
                            taskRunID: runContext.runID,
                            taskTitle: "Agent 群聊 · \(launch.displayName)"
                        ),
                        adapterSessionID: adapterSessionID
                    )
                    do {
                        providers.append(try NativeAgentPluginToolProvider(
                            service: self,
                            runtimeStore: pluginRuntimeStore,
                            identity: identity,
                            tools: tools,
                            displayName: launch.displayName,
                            ownerUserID: ownerUserID,
                            projectRootURL: resolvedProject.absoluteURL,
                            workspaceID: resolvedProject.workspace.id,
                            permissionSnapshot: permissionSnapshot
                        ))
                    } catch {
                        _ = await pluginRuntimeStore.cancel(
                            adapterSessionID: adapterSessionID,
                            invocationID: nil
                        )
                        throw error
                    }
                } catch {
                    await client.terminate()
                    try? FileManager.default.removeItem(at: launch.visualSessionURL)
                    throw error
                }
            }
        }
        return providers
    }

    /// Creates a compact, run-scoped capability broker. It exposes only discovery/invocation
    /// tools to the model and starts a concrete Plugin only after the Agent selects it.
    public func makeAgentCapabilityToolProvider(
        ownerUserID: String,
        runContext: LocalAgentChatRunContext,
        projectContext: LocalConnectorPluginApplicationContext,
        executionPlan: LocalAgentTodoExecutionPlan
    ) throws -> any AgentToolProvider {
        guard state.user?.id == ownerUserID,
              runContext.ownerUserID == ownerUserID,
              projectContext.projectID == runContext.projectID,
              let projectRoot = projectContext.projectRoot else {
            throw NativePluginRuntimeError.invalidRequest("本地 Agent 能力目录与当前账户或项目不匹配")
        }
        guard runContext.lane == .executor else {
            throw NativePluginRuntimeError.invalidRequest("通讯线程不能装配项目文件、终端或 Plugin 能力")
        }
        try executionPlan.validate()
        let resolvedProject = try resolveProjectPath(projectRoot)
        let installed = try installedAgentPlugins(ownerUserID: ownerUserID)
        let installedByID = Dictionary(uniqueKeysWithValues: installed.map { ($0.id, $0) })
        let selectedPlugins = try executionPlan.plugins.map { selection in
            guard let plugin = installedByID[selection.pluginID] else {
                throw NativePluginRuntimeError.invalidRequest(
                    "Todo 所需 Plugin「\(selection.displayName)」已经卸载或停用，请由通讯线程调整任务计划。"
                )
            }
            return plugin
        }
        return NativeAgentCapabilityToolProvider(
            service: self,
            ownerUserID: ownerUserID,
            runContext: runContext,
            projectContext: projectContext,
            resolvedProject: resolvedProject,
            builtinCapabilities: Set(executionPlan.builtinCapabilities),
            installedPlugins: selectedPlugins
        )
    }

    func approveAgentPluginTool(
        callID: String,
        componentKey: String,
        toolName: String,
        arguments: NativeJSONValue,
        policy: NativePluginToolPolicy,
        projectRootURL: URL,
        workspaceID: String
    ) async -> Bool {
        guard policy.approvalMode == "per_call" else { return true }
        let summary = Self.safeArgumentSummary(toolName: toolName, arguments: arguments)
        let requiredPermissions = policy.requiredPermissions(for: arguments)
        let decision = await approvalDecision(
            requestID: callID,
            command: "agent_plugin:\(componentKey) · \(toolName)",
            arguments: [summary],
            cwd: projectRootURL,
            projectRoot: projectRootURL,
            source: "plugin_agent_group_chat",
            risk: .init(
                level: policy.riskLevel,
                reason: "本地群聊 Agent 请求执行 Plugin 操作：\(summary)"
            ),
            requestedPermissionsDescription: Self.permissionDescription(
                toolName: toolName,
                requiredPermissions: requiredPermissions
            ),
            approvalScopeKey: "agent-plugin:\(componentKey)",
            workspaceID: workspaceID
        )
        if case .approve = decision { return true }
        return false
    }

    private static func agentPluginManifest(
        record: NativeInstalledPluginRecord
    ) throws -> NativePluginManifest {
        let url = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .appendingPathComponent("chatos.plugin.json")
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(contentsOf: url, options: .mappedIfSafe)
        )
        guard manifest.schemaVersion == 3,
              manifest.version == record.version,
              !manifest.mcpServers.isEmpty else {
            throw NativePluginRuntimeError.invalidManifest("Plugin 没有可用于 Agent 的 MCP 组件")
        }
        return manifest
    }

    private static func validatedAgentPluginTools(
        _ tools: [NativeJSONValue]
    ) throws -> [NativeJSONValue] {
        guard (1...200).contains(tools.count) else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP 工具数量无效")
        }
        var names = Set<String>()
        for tool in tools {
            guard let object = tool.jsonObject,
                  let name = object["name"]?.jsonString?
                    .trimmingCharacters(in: .whitespacesAndNewlines),
                  !name.isEmpty,
                  names.insert(name).inserted,
                  object["inputSchema"]?.jsonObject != nil else {
                throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP 工具定义无效")
            }
        }
        return tools.sorted {
            ($0.jsonObject?["name"]?.jsonString ?? "")
                < ($1.jsonObject?["name"]?.jsonString ?? "")
        }
    }
}

/// Built-in Skill used by local Agents to keep the normal tool surface small. Installed Plugin
/// metadata, schemas and processes are revealed lazily and only for the current run.
enum LocalAgentCapabilityDiscoverySkill {
    static let instructions = """
    <skill name="chatos-capability-discovery">
    当任务需要 Relay 之外的本机工具、项目文件或 Plugin 时，按以下顺序工作：
    1. 使用 capability_search，用简短任务关键词搜索能力；不要为了探索而列出全部能力。
    2. 只对最匹配的一个 plugin_option 调用 capability_describe，读取它在本轮可用的工具和参数。项目团队可在这里发现 ChatOS 内置的项目文件与终端 MCP；独立私聊不会获得项目能力。
    3. 使用 capability_invoke 调用选中的 tool_option。只有需要另一类能力时才继续搜索。
    4. 能力、项目 ID、项目根目录和本机授权由 ChatOS 内部绑定；不得猜测、索要或回显这些内部值。
    5. 文件操作默认限定在当前项目；写入、删除、计费或其他高风险动作仍可能要求 Human 确认。
    </skill>
    """
}

private actor NativeAgentCapabilityToolProvider: AgentToolProvider {
    static let searchToolName = "capability_search"
    static let describeToolName = "capability_describe"
    static let invokeToolName = "capability_invoke"

    private enum CapabilityKind: Sendable {
        case builtIn
        case plugin(NativeInstalledAgentPlugin)
    }

    private struct CapabilityOption: Sendable {
        let token: String
        let name: String
        let description: String
        let kind: CapabilityKind
    }

    private struct SearchArguments: Decodable {
        let query: String?
    }

    private struct SelectionArguments: Decodable {
        let pluginOption: String
    }

    private struct InvokeArguments: Decodable {
        let pluginOption: String
        let toolOption: String
        let arguments: NativeJSONValue
    }

    private struct PluginSummary: Encodable {
        let pluginOption: String
        let name: String
        let description: String
    }

    private struct SearchResponse: Encodable {
        let matches: [PluginSummary]
    }

    private struct ToolSummary: Encodable {
        let toolOption: String
        let name: String
        let description: String
        let inputSchema: NativeJSONValue
        let effect: String
    }

    private struct DescribeResponse: Encodable {
        let pluginOption: String
        let name: String
        let tools: [ToolSummary]
    }

    private let service: NativeLocalConnectorService
    private let ownerUserID: String
    private let runContext: LocalAgentChatRunContext
    private let projectContext: LocalConnectorPluginApplicationContext
    private let resolvedProject: NativeResolvedProjectPath
    private let builtinCapabilities: Set<LocalAgentTodoBuiltinCapability>
    private let options: [CapabilityOption]
    private var registries: [String: AgentToolProviderRegistry] = [:]
    private var toolNamesByOption: [String: [String: String]] = [:]

    init(
        service: NativeLocalConnectorService,
        ownerUserID: String,
        runContext: LocalAgentChatRunContext,
        projectContext: LocalConnectorPluginApplicationContext,
        resolvedProject: NativeResolvedProjectPath,
        builtinCapabilities: Set<LocalAgentTodoBuiltinCapability>,
        installedPlugins: [NativeInstalledAgentPlugin]
    ) {
        self.service = service
        self.ownerUserID = ownerUserID
        self.runContext = runContext
        self.projectContext = projectContext
        self.resolvedProject = resolvedProject
        self.builtinCapabilities = builtinCapabilities
        let builtinOptions: [CapabilityOption] = builtinCapabilities.isEmpty ? [] : [
            .init(
                token: "builtin_1",
                name: "ChatOS 项目文件与终端",
                description: "本 Todo 已批准的项目基础能力：\(builtinCapabilities.map(\.rawValue).sorted().joined(separator: ", "))。真实项目路径由客户端绑定。",
                kind: .builtIn
            ),
        ]
        self.options = builtinOptions + installedPlugins.enumerated().map { offset, plugin in
            .init(
                token: "plugin_\(offset + 1)",
                name: plugin.displayName,
                description: plugin.description,
                kind: .plugin(plugin)
            )
        }
    }

    func definitions() async throws -> [AgentToolDefinition] {
        [
            .init(
                name: Self.searchToolName,
                description: "按任务关键词搜索本机已安装能力。只返回匹配 Plugin 的本轮临时选项和简介，不启动 Plugin，也不展开全部工具。",
                schema: Data(#"{"type":"object","properties":{"query":{"type":"string","minLength":1,"maxLength":200}},"required":["query"],"additionalProperties":false}"#.utf8)
            ),
            .init(
                name: Self.describeToolName,
                description: "按 capability_search 返回的临时 plugin_option，惰性启动一个 Plugin，并读取它在本轮可用的工具说明。",
                schema: Data(#"{"type":"object","properties":{"plugin_option":{"type":"string","minLength":1,"maxLength":80}},"required":["plugin_option"],"additionalProperties":false}"#.utf8)
            ),
            .init(
                name: Self.invokeToolName,
                description: "调用已经通过 capability_describe 展开的一个工具。plugin_option 和 tool_option 都必须使用本轮临时选项；真实 Plugin、项目和路径上下文由客户端内部绑定。",
                schema: Data(#"{"type":"object","properties":{"plugin_option":{"type":"string","minLength":1,"maxLength":80},"tool_option":{"type":"string","minLength":1,"maxLength":80},"arguments":{"type":"object"}},"required":["plugin_option","tool_option","arguments"],"additionalProperties":false}"#.utf8),
                effect: .write
            ),
        ]
    }

    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        switch call.name {
        case Self.searchToolName:
            let arguments = try decode(SearchArguments.self, from: call.arguments)
            let query = arguments.query?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            guard !query.isEmpty else {
                return .failure("请提供当前任务需要的能力关键词，不要枚举全部 Plugin。")
            }
            let terms = query.lowercased().split(whereSeparator: { $0.isWhitespace }).map(String.init)
            let matches = options.filter { option in
                let builtInKeywords = if case .builtIn = option.kind {
                    " 文件 读写 代码 终端 shell command filesystem"
                } else {
                    ""
                }
                let searchable = "\(option.name) \(option.description)\(builtInKeywords)".lowercased()
                return terms.contains(where: searchable.contains)
            }.prefix(12).map { option in
                PluginSummary(
                    pluginOption: option.token,
                    name: option.name,
                    description: option.description
                )
            }
            return try outcome(SearchResponse(matches: Array(matches)))

        case Self.describeToolName:
            let arguments = try decode(SelectionArguments.self, from: call.arguments)
            guard let option = options.first(where: { $0.token == arguments.pluginOption }) else {
                return .failure("能力选项无效或已经过期，请重新搜索。")
            }
            let registry = try await registry(for: option)
            var names: [String: String] = [:]
            let tools = try registry.definitions.enumerated().map { offset, definition in
                let token = "tool_\(offset + 1)"
                names[token] = definition.name
                let schemaText = redact(String(decoding: definition.schema, as: UTF8.self))
                let schema = try JSONDecoder().decode(NativeJSONValue.self, from: Data(schemaText.utf8))
                return ToolSummary(
                    toolOption: token,
                    name: definition.name,
                    description: redact(definition.description),
                    inputSchema: schema,
                    effect: definition.effect.rawValue
                )
            }
            toolNamesByOption[option.token] = names
            return try outcome(DescribeResponse(
                pluginOption: option.token,
                name: option.name,
                tools: tools
            ))

        case Self.invokeToolName:
            let arguments = try decode(InvokeArguments.self, from: call.arguments)
            guard let option = options.first(where: { $0.token == arguments.pluginOption }),
                  let toolName = toolNamesByOption[option.token]?[arguments.toolOption] else {
                return .failure("请先搜索并查看该能力，再使用本轮返回的工具选项调用。")
            }
            guard arguments.arguments.jsonObject != nil else {
                return .failure("能力工具参数必须是 JSON 对象。")
            }
            let registry = try await registry(for: option)
            let result = try await registry.execute(.init(
                id: call.id,
                name: toolName,
                arguments: arguments.arguments.canonicalJSONString
            ))
            return .init(
                redact(result.content),
                madeProgress: result.madeProgress,
                isError: result.isError
            )

        default:
            return .failure("能力发现工具不可用：\(call.name)")
        }
    }

    private func registry(for option: CapabilityOption) async throws -> AgentToolProviderRegistry {
        if let existing = registries[option.token] { return existing }
        let providers: [any AgentToolProvider]
        switch option.kind {
        case .builtIn:
            providers = [NativeAgentBuiltinToolProvider(
                service: service,
                runContext: runContext,
                resolvedProject: resolvedProject,
                allowedCapabilities: builtinCapabilities
            )]
        case let .plugin(plugin):
            providers = try await service.makeAgentPluginToolProviders(
                ownerUserID: ownerUserID,
                runContext: runContext,
                pluginIDs: [plugin.id],
                projectContext: projectContext
            )
        }
        let registry = try await AgentToolProviderRegistry(providers: providers)
        registries[option.token] = registry
        return registry
    }

    private func decode<Value: Decodable>(_ type: Value.Type, from json: String) throws -> Value {
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            return try decoder.decode(Value.self, from: Data(json.utf8))
        } catch {
            throw NativePluginRuntimeError.invalidRequest("能力发现工具参数无效")
        }
    }

    private func outcome<Value: Encodable>(_ value: Value) throws -> AgentToolOutcome {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return .init(redact(String(decoding: try encoder.encode(value), as: UTF8.self)))
    }

    private func redact(_ value: String) -> String {
        value.replacingOccurrences(of: runContext.projectID, with: "[internal-project]")
    }
}

private struct NativeAgentBuiltinToolProvider: AgentToolProvider, Sendable {
    private let service: NativeLocalConnectorService
    private let runContext: LocalAgentChatRunContext
    private let resolvedProject: NativeResolvedProjectPath
    private let allowedCapabilities: Set<LocalAgentTodoBuiltinCapability>

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
    }

    func definitions() async throws -> [AgentToolDefinition] {
        try Self.nativeDefinitions.filter { value in
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
            if NativeMCPCodeWriteStore.toolNames.contains(name) {
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
                effect: effect
            )
        }
    }

    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        do {
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
            return .init(result.canonicalJSONString)
        } catch {
            return .failure(error.localizedDescription)
        }
    }

    private static let nativeDefinitions = NativeMCPCodeReadTools.toolDefinitions
        + NativeMCPCodeWriteStore.toolDefinitions
        + NativeMCPTerminalStore.toolDefinitions

    private static func capability(for toolName: String) -> LocalAgentTodoBuiltinCapability {
        if NativeMCPCodeWriteStore.toolNames.contains(toolName) { return .projectWrite }
        if NativeMCPTerminalStore.toolNames.contains(toolName) { return .terminal }
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
        if NativeMCPCodeReadTools.toolDefinitions.contains(where: {
            $0.jsonObject?["name"]?.jsonString == name
        }) {
            return try await Task.detached {
                try NativeMCPCodeReadTools(
                    workspace: resolvedProject.workspace,
                    projectRoot: projectRoot,
                    requestCWD: nil,
                    defaultToolRoot: nil
                ).call(name: name, arguments: arguments)
            }.value
        }
        if NativeMCPCodeWriteStore.toolNames.contains(name) {
            if name == "commit_edit_session" {
                let decision = await approvalDecision(
                    requestID: callID,
                    command: "agent_project_file_commit",
                    arguments: ["提交当前 Agent 暂存的项目文件修改"],
                    cwd: projectRoot,
                    projectRoot: projectRoot,
                    source: "local-agent-builtin-mcp",
                    risk: .init(level: "medium", reason: "Agent 将修改当前项目中的文件。"),
                    approvalScopeKey: "agent-project-files",
                    workspaceID: resolvedProject.workspace.id
                )
                guard case .approve = decision else {
                    throw NativePluginRuntimeError.invalidRequest("用户未批准 Agent 修改项目文件")
                }
            }
            return try await mcpCodeWriteStore.call(
                name: name,
                arguments: arguments,
                scope: .init(
                    workspaceID: resolvedProject.workspace.id,
                    sessionID: runContext.roomID,
                    runID: runContext.runID
                ),
                projectRoot: projectRoot
            )
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
        return try await mcpTerminalStore.execute(
            command: command,
            cwd: cwd,
            projectRoot: projectRoot,
            background: arguments["background"]?.jsonBool ?? false
        )
    }
}

private struct NativeAgentPluginToolProvider: AgentToolProvider, Sendable {
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
        let rawResult = try await runtimeStore.call(
            adapterSessionID: identity.adapterSessionID,
            invocationID: call.id,
            toolName: call.name,
            arguments: arguments,
            timeout: .milliseconds(policy.timeoutMilliseconds)
        )
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
