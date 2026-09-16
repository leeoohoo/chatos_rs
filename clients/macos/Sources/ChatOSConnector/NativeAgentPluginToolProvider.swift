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
