import ChatOSAgentRuntime
import ChatOSCore
import Foundation

struct NativeAgentPluginExecutionPresentation: Sendable {
    let approvalSource: String
    let approvalReasonPrefix: String
    let taskTitlePrefix: String

    static let agentGroupChat = Self(
        approvalSource: "plugin_agent_group_chat",
        approvalReasonPrefix: "本地群聊 Agent 请求执行 Plugin 操作",
        taskTitlePrefix: "Agent 群聊"
    )
    static let localTaskExecution = Self(
        approvalSource: "local_agent_task_execution",
        approvalReasonPrefix: "本地任务请求执行 Plugin 操作",
        taskTitlePrefix: "本地任务"
    )
}

extension NativeLocalConnectorService {
    func cancelAgentPluginTools(runID: String) async {
        await pluginRuntimeStore.cancel(runID: runID)
    }

    func makeTaskExecutionCapabilityToolProvider(
        ownerUserID: String,
        runID: String,
        conversationID: String,
        projectContext: LocalConnectorPluginApplicationContext,
        executionRootURL: URL,
        workspaceScopeID: String,
        builtinCapabilities: Set<LocalAgentTodoBuiltinCapability> = [],
        pluginSelectionIDs: [String],
        pluginReleaseSnapshots: [String: NativeLocalAgentTaskPluginReleaseSnapshot] = [:],
        productSkillBundle: LocalAgentSkillBundleSnapshot? = nil,
        selectedExternalMCPIDs: Set<String> = [],
        preactivatedSkillRefs: Set<String> = []
    ) async throws -> any AgentToolProvider {
        guard state.user?.id == ownerUserID else {
            throw NativePluginRuntimeError.invalidRequest(
                "本地任务 Plugin 与当前账户不匹配"
            )
        }
        let projectID = projectContext.projectID ?? "direct:\(conversationID)"
        let resolvedProject: NativeResolvedProjectPath?
        if let projectRoot = projectContext.projectRoot {
            guard projectContext.projectID != nil else {
                throw NativePluginRuntimeError.invalidRequest("本地任务 Plugin 项目上下文无效")
            }
            resolvedProject = try resolveProjectPath(projectRoot)
        } else {
            guard projectContext.projectID == nil else {
                throw NativePluginRuntimeError.invalidRequest("本地任务 Plugin 缺少项目根目录")
            }
            resolvedProject = nil
        }
        let runContext = try LocalAgentChatRunContext(
            ownerUserID: ownerUserID,
            projectID: projectID,
            roomID: conversationID,
            agentID: "local-task-execution",
            deliveryID: runID,
            triggerMessageID: runID,
            rootMessageID: runID,
            runID: runID,
            hopCount: 0,
            lane: .executor
        )
        let installedPlugins = try await installedAgentPlugins(ownerUserID: ownerUserID)
        let selectedPlugins: [NativeInstalledAgentPlugin]
        let installedByKey = Dictionary(
            uniqueKeysWithValues: installedPlugins.map {
                (NativeLocalAgentTaskToolAuthorization.pluginSelectionID(pluginKey: $0.pluginKey, componentKey: $0.componentKey), $0)
            }
        )
        selectedPlugins = try Array(Set(pluginSelectionIDs)).sorted().map { selectionID in
            guard let plugin = installedByKey[selectionID] else {
                throw NativePluginRuntimeError.invalidRequest(
                    "本地任务选择的 Plugin component 未安装或已停用"
                )
            }
            return plugin
        }
        guard Set(pluginReleaseSnapshots.keys) == Set(pluginSelectionIDs) else {
            throw NativePluginRuntimeError.invalidRequest(
                "本地任务 Plugin Release 快照与授权范围不一致"
            )
        }
        for plugin in selectedPlugins {
            let selectionID = NativeLocalAgentTaskToolAuthorization.pluginSelectionID(pluginKey: plugin.pluginKey, componentKey: plugin.componentKey)
            guard let expected = pluginReleaseSnapshots[selectionID],
                      expected.pluginID == plugin.pluginID,
                      expected.componentKey == plugin.componentKey,
                      let record = state.installedPluginRecords?[plugin.pluginID],
                      record.releaseID == expected.releaseID,
                      record.version == expected.version,
                      record.artifactSHA256 == expected.artifactSHA256,
                      (record.pluginKey?.trimmingCharacters(in: .whitespacesAndNewlines)
                        .nilIfEmpty ?? record.pluginID) == expected.pluginKey else {
                    throw NativePluginRuntimeError.invalidRequest(
                        "本地任务固定的 Plugin Release 已变更，请重新创建任务"
                    )
            }
        }
        return NativeAgentCapabilityToolProvider(
            service: self,
            ownerUserID: ownerUserID,
            runContext: runContext,
            projectContext: projectContext,
            resolvedProject: resolvedProject,
            fallbackWorkspaceID: workspaceScopeID,
            fallbackWorkspaceRootURL: executionRootURL,
            builtinCapabilities: builtinCapabilities,
            installedPlugins: selectedPlugins,
            executionPresentation: .localTaskExecution,
            fallbackToSelectedOptions: true,
            invokeBuiltinsThroughBroker: false,
            productSkillBundle: productSkillBundle,
            selectedExternalMCPIDs: selectedExternalMCPIDs,
            preactivatedSkillRefs: preactivatedSkillRefs
        )
    }

    func taskExecutionConversationRoot(
        ownerUserID: String,
        conversationID: String
    ) throws -> URL {
        guard state.user?.id == ownerUserID else {
            throw NativePluginRuntimeError.invalidRequest("本地任务与当前账户不匹配")
        }
        let ownerScope = NativePluginManifestLoader.sha256(ownerUserID)
        let conversationScope = NativePluginManifestLoader.sha256(conversationID)
        let root = configuration.stateURL
            .deletingLastPathComponent()
            .appendingPathComponent("LocalAgent/ConversationWorkspaces", isDirectory: true)
            .appendingPathComponent(ownerScope, isDirectory: true)
            .appendingPathComponent(conversationScope, isDirectory: true)
        try FileManager.default.createDirectory(
            at: root,
            withIntermediateDirectories: true
        )
        return root.standardizedFileURL
    }
}

/// Discovery has no execution backend: durable Task tools own all builtin mutations.
struct NativeLocalTaskBuiltinDiscoveryProvider: AgentToolProvider, Sendable {
    let allowedCapabilities: Set<LocalAgentTodoBuiltinCapability>

    func definitions() async throws -> [AgentToolDefinition] {
        try NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools.compactMap { value in
            guard case let .object(tool) = value,
                  case let .string(name)? = tool["name"],
                  case let .string(description)? = tool["description"],
                  let schema = tool["parameters"] else { return nil }
            let capability: LocalAgentTodoBuiltinCapability
            let effect: AgentToolDefinition.Effect
            let providerID: String
            if NativeMCPCodeWriteStore.toolNames.contains(name) {
                capability = .projectWrite
                effect = .write
                providerID = ProductToolProviderID.projectWrite
            } else if NativeLocalAgentPlatformToolCatalog.taskExecutionTerminalToolNames.contains(name) {
                capability = .terminal
                effect = .terminal
                providerID = ProductToolProviderID.terminal
            } else if ["read_file_raw", "read_file_range", "list_dir", "search_text", "read_file", "search_files"].contains(name) {
                capability = .projectRead
                effect = .readOnly
                providerID = ProductToolProviderID.projectRead
            } else {
                return nil
            }
            guard allowedCapabilities.contains(capability),
                  let binding = ToolSkillCoverageCatalog.product.bindings.first(where: {
                      $0.providerID == providerID && $0.toolNames.contains(name)
                  }) else { return nil }
            return .init(
                name: name,
                description: description,
                schema: try JSONEncoder().encode(schema),
                effect: effect,
                providerID: binding.providerID,
                skillBindingID: binding.id
            )
        }
    }

    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        .failure("请通过 Task 原生工具直接调用 `\(call.name)`。")
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
