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
            try Task.checkCancellation()
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
            try Task.checkCancellation()
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
