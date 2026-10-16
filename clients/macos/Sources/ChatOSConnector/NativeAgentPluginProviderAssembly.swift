import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// Starts already-installed stdio MCP plugins directly from the client for one local Agent run.
/// It deliberately bypasses relay request signing because there is no remote caller: account,
/// project, Agent and plugin selection have already been fixed by local application state.
extension NativeLocalConnectorService {
    public func installedAgentPlugins(
        ownerUserID: String
    ) async throws -> [NativeInstalledAgentPlugin] {
        guard state.user?.id == ownerUserID else {
            throw NativePluginRuntimeError.invalidRequest("本机 Plugin 不属于当前账户")
        }
        await repairLegacyPluginIntegrityRecordsIfNeeded()
        return (state.installedPluginRecords ?? [:]).values.flatMap { record -> [NativeInstalledAgentPlugin] in
            guard state.pluginPreferences[record.pluginID] ?? true,
                  NativePluginInstallationIntegrity.status(record: record) == .verified,
                  let manifest = try? Self.agentPluginManifest(record: record) else { return [] }
            return manifest.mcpServers.keys.sorted().compactMap { componentKey in
                guard let server = manifest.mcpServers[componentKey] else { return nil }
                return .init(
                    pluginID: record.pluginID,
                    pluginKey: record.pluginKey,
                    componentKey: componentKey,
                    displayName: server.agent?.displayName
                        ?? manifest.interface?.displayName
                        ?? manifest.name,
                    description: server.agent?.description ?? manifest.description
                )
            }
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
        plugins: [NativeInstalledAgentPlugin],
        projectContext: LocalConnectorPluginApplicationContext
    ) async throws -> [any AgentToolProvider] {
        try await makeAgentPluginSkillToolProviders(
            ownerUserID: ownerUserID,
            runContext: runContext,
            plugins: plugins,
            projectContext: projectContext
        ).map { $0 as any AgentToolProvider }
    }

    func makeAgentPluginSkillToolProviders(
        ownerUserID: String,
        runContext: LocalAgentChatRunContext,
        plugins: [NativeInstalledAgentPlugin],
        projectContext: LocalConnectorPluginApplicationContext,
        fallbackWorkspaceID: String? = nil,
        fallbackWorkspaceRootURL: URL? = nil,
        executionPresentation: NativeAgentPluginExecutionPresentation = .agentGroupChat
    ) async throws -> [NativeAgentPluginToolProvider] {
        guard state.user?.id == ownerUserID,
              runContext.ownerUserID == ownerUserID,
              let deviceID = state.deviceID else {
            throw NativePluginRuntimeError.invalidRequest("本地 Agent Plugin 与当前账户或项目不匹配")
        }
        let resolvedProject: NativeResolvedProjectPath?
        if let rawProjectRoot = projectContext.projectRoot {
            guard projectContext.projectID == runContext.projectID else {
                throw NativePluginRuntimeError.invalidRequest("本地 Agent Plugin 与当前项目不匹配")
            }
            resolvedProject = try resolveProjectPath(rawProjectRoot)
        } else {
            guard projectContext.projectID == nil,
                  runContext.projectID.hasPrefix("direct:") else {
                throw NativePluginRuntimeError.invalidRequest("本地 Agent Plugin 缺少项目上下文")
            }
            resolvedProject = nil
        }
        guard let workspaceRootURL = resolvedProject?.absoluteURL ?? fallbackWorkspaceRootURL,
              let workspaceID = resolvedProject?.workspace.id ?? fallbackWorkspaceID else {
            throw NativePluginRuntimeError.invalidRequest("本地 Agent Plugin 缺少工作目录")
        }
        let selectedPlugins = plugins.sorted { $0.id < $1.id }
        guard Set(selectedPlugins.map(\.id)).count == selectedPlugins.count,
              selectedPlugins.count <= 100 else {
            throw NativePluginRuntimeError.invalidRequest("本地 Agent 选择的 Plugin 数量无效")
        }
        var providers: [NativeAgentPluginToolProvider] = []
        for plugin in selectedPlugins {
            let pluginID = plugin.pluginID
            guard state.pluginPreferences[pluginID] ?? true,
                  let record = state.installedPluginRecords?[pluginID] else {
                throw NativePluginRuntimeError.invalidRequest("Agent 配置的 Plugin 未安装或已停用：\(pluginID)")
            }
            let manifest = try Self.agentPluginManifest(record: record)
            guard manifest.mcpServers[plugin.componentKey] != nil else {
                throw NativePluginRuntimeError.invalidRequest("Agent 配置的 Plugin component 不存在：\(plugin.componentKey)")
            }
            let installationURL = URL(
                fileURLWithPath: record.installationPath,
                isDirectory: true
            )
            let skillSnapshot = try PluginSkillSnapshot.load(
                installationRoot: installationURL,
                relativeSkillDirectories: manifest.skills.map(\.path)
            )
            let skillSession = PluginToolSkillSession(snapshot: skillSnapshot)
            let permissionSnapshot = Set(manifest.permissions.map(\.permission))
            let componentKey = plugin.componentKey
                let adapterSessionID = "agent-" + UUID().uuidString.lowercased()
                if componentKey == NativeBrowserPluginIdentity.componentKey {
                    await browserExtensionPairingRuntime.stop()
                }
                let launch = try NativePluginManifestLoader.prepare(
                    record: record,
                    manifest: manifest,
                    componentKey: componentKey,
                    serverKey: componentKey,
                    adapterSessionID: adapterSessionID,
                    ownerUserID: ownerUserID,
                    deviceID: deviceID,
                    workspaceID: workspaceID,
                    workspaceRoot: workspaceRootURL,
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
                        projectID: projectContext.projectID,
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
                        projectRootURL: workspaceRootURL,
                        workspaceID: workspaceID
                    )
                    await pluginRuntimeStore.bindOwner(
                        .init(
                            conversationID: runContext.roomID,
                            sourceUserMessageID: runContext.triggerMessageID,
                            taskID: runContext.deliveryID,
                            taskRunID: runContext.runID,
                            taskTitle: "\(executionPresentation.taskTitlePrefix) · \(launch.displayName)"
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
                            projectRootURL: workspaceRootURL,
                            workspaceID: workspaceID,
                            permissionSnapshot: permissionSnapshot,
                            executionPresentation: executionPresentation,
                            pluginSkillSnapshot: skillSnapshot,
                            pluginSkillSession: skillSession
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
        return providers
    }
    /// Creates a compact, run-scoped capability broker. It exposes only discovery/invocation
    /// tools to the model and starts a concrete Plugin only after the Agent selects it.
    public func makeAgentCapabilityToolProvider(
        ownerUserID: String,
        runContext: LocalAgentChatRunContext,
        projectContext: LocalConnectorPluginApplicationContext,
        executionPlan: LocalAgentTodoExecutionPlan
    ) async throws -> any AgentToolProvider {
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
        let installed = try await installedAgentPlugins(ownerUserID: ownerUserID)
        let installedByID = Dictionary(uniqueKeysWithValues: installed.map { ($0.id, $0) })
        let selectedPlugins = try executionPlan.plugins.map { selection in
            guard let plugin = installedByID["\(selection.pluginID):\(selection.componentKey)"] else {
                throw NativePluginRuntimeError.invalidRequest(
                    "Todo 所需 Plugin component「\(selection.displayName)」已经卸载或停用，请由通讯线程调整任务计划。"
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
        workspaceID: String,
        executionPresentation: NativeAgentPluginExecutionPresentation
    ) async -> Bool {
        guard policy.approvalMode == "per_call" else { return true }
        let summary = Self.safeArgumentSummary(toolName: toolName, arguments: arguments)
        let evaluationSummary = Self.safeApprovalEvaluationSummary(
            toolName: toolName,
            arguments: arguments
        )
        let requiredPermissions = policy.requiredPermissions(for: arguments)
        if state.approvalMode == .autoApproval,
           let reason = Self.deterministicAutoApprovalReason(
               componentKey: componentKey,
               toolName: toolName,
               arguments: arguments,
               requiredPermissions: requiredPermissions
           ) {
            publishApprovalEvent(.init(
                requestID: callID,
                command: "agent_plugin:\(componentKey) · \(toolName) \(summary)",
                cwd: projectRootURL.path,
                source: executionPresentation.approvalSource,
                risk: policy.riskLevel,
                decision: "approved",
                reason: reason,
                mode: .autoApproval,
                reviewer: .policy
            ))
            return true
        }
        let decision = await approvalDecision(
            requestID: callID,
            command: "agent_plugin:\(componentKey) · \(toolName)",
            arguments: [summary],
            cwd: projectRootURL,
            projectRoot: projectRootURL,
            source: executionPresentation.approvalSource,
            risk: .init(
                level: policy.riskLevel,
                reason: "\(executionPresentation.approvalReasonPrefix)：\(summary)"
            ),
            requestedPermissionsDescription: Self.permissionDescription(
                toolName: toolName,
                requiredPermissions: requiredPermissions
            ),
            approvalScopeKey: "agent-plugin:\(componentKey):\(toolName):\(requiredPermissions.sorted().joined(separator: ","))",
            approvalSessionReuse: .declaredScope,
            evaluationArguments: [evaluationSummary],
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
        let manifest = try NativePluginManifestLoader.loadManifest(from: url)
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
