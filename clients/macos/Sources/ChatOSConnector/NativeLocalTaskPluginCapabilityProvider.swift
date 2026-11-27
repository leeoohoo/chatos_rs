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
        approvalReasonPrefix: "任务请求执行 Plugin 操作",
        taskTitlePrefix: "任务"
    )
}

extension NativeLocalConnectorService {
    func cancelAgentPluginTools(runID: String) async {
        await pluginRuntimeStore.cancel(runID: runID)
    }

    public func agentPluginActivity(runID: String) async -> MessageTaskRunActivity? {
        await pluginRuntimeStore.invocationActivity(runID: runID)
    }

    public func agentPluginActivities(
        runIDs: Set<String>
    ) async -> [String: MessageTaskRunActivity] {
        await pluginRuntimeStore.invocationActivities(runIDs: runIDs)
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
        preactivatedSkillRefs: Set<String> = [],
        restoredCapabilityDisclosures: Set<LocalRunCapabilityDisclosure> = []
    ) async throws -> any AgentToolProvider {
        guard state.user?.id == ownerUserID else {
            throw NativePluginRuntimeError.invalidRequest(
                "任务 Plugin 与当前账户不匹配"
            )
        }
        let projectID = projectContext.projectID ?? "direct:\(conversationID)"
        let resolvedProject: NativeResolvedProjectPath?
        if let projectRoot = projectContext.projectRoot {
            guard projectContext.projectID != nil else {
                throw NativePluginRuntimeError.invalidRequest("任务 Plugin 项目上下文无效")
            }
            let source = try resolveProjectPath(projectRoot)
            if source.absoluteURL.standardizedFileURL.resolvingSymlinksInPath()
                == executionRootURL.standardizedFileURL.resolvingSymlinksInPath() {
                resolvedProject = source
            } else {
                var workspace = source.workspace
                workspace.id = workspaceScopeID
                workspace.absoluteRoot = executionRootURL.path
                workspace.fingerprint = NativePluginManifestLoader.sha256(
                    workspaceScopeID + "\n" + executionRootURL.path
                )
                resolvedProject = .init(
                    workspace: workspace,
                    relativePath: ".",
                    absoluteURL: executionRootURL,
                    logicalPrefix: nil
                )
            }
        } else {
            guard projectContext.projectID == nil else {
                throw NativePluginRuntimeError.invalidRequest("任务 Plugin 缺少项目根目录")
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
        if let productSkillBundle {
            try NativeLocalTaskBuiltinDiscoveryProvider.validateFrozenSkillPlan(
                allowedCapabilities: builtinCapabilities,
                bundle: productSkillBundle
            )
        }
        let installedPlugins = try await installedAgentPlugins(ownerUserID: ownerUserID)
        let installedByKey = Dictionary(
            uniqueKeysWithValues: installedPlugins.map {
                (NativeLocalAgentTaskToolAuthorization.pluginSelectionID(pluginKey: $0.pluginKey, componentKey: $0.componentKey), $0)
            }
        )
        guard Set(pluginReleaseSnapshots.keys) == Set(pluginSelectionIDs) else {
            throw NativePluginRuntimeError.invalidRequest(
                "任务 Plugin Release 快照与授权范围不一致"
            )
        }
        var pluginSelections: [NativeAgentPluginCapabilitySelection] = []
        for selectionID in Array(Set(pluginSelectionIDs)).sorted() {
            guard let expected = pluginReleaseSnapshots[selectionID] else {
                throw NativePluginRuntimeError.invalidRequest(
                    "任务 Plugin Release 快照与授权范围不一致"
                )
            }
            guard let plugin = installedByKey[selectionID] else {
                pluginSelections.append(.unavailable(
                    pluginKey: expected.pluginKey,
                    componentKey: expected.componentKey,
                    reason: "该任务冻结的 Plugin 已卸载或停用。其他已授权能力仍可继续；若验收必须依赖此 Plugin，请通过 replan_required 创建使用当前已安装能力的新规划任务，不要重复调用或向用户转嫁内部恢复。"
                ))
                continue
            }
            guard expected.pluginID == plugin.pluginID,
                  expected.componentKey == plugin.componentKey,
                  let record = state.installedPluginRecords?[plugin.pluginID],
                  record.releaseID == expected.releaseID,
                  record.version == expected.version,
                  record.artifactSHA256 == expected.artifactSHA256,
                  (record.pluginKey?.trimmingCharacters(in: .whitespacesAndNewlines)
                    .nilIfEmpty ?? record.pluginID) == expected.pluginKey else {
                pluginSelections.append(.unavailable(
                    pluginKey: expected.pluginKey,
                    componentKey: expected.componentKey,
                    reason: "该任务冻结的 Plugin Release 已变更，旧快照不会被新版本替代。其他已授权能力仍可继续；若验收必须依赖此 Plugin，请通过 replan_required 创建绑定当前 Release 的新规划任务，不要重复调用或向用户转嫁内部恢复。"
                ))
                continue
            }
            pluginSelections.append(.available(plugin))
        }
        let selectedPlugins = pluginSelections.compactMap { selection -> NativeInstalledAgentPlugin? in
            guard case let .available(plugin) = selection else { return nil }
            return plugin
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
            pluginSelections: pluginSelections,
            executionPresentation: .localTaskExecution,
            fallbackToSelectedOptions: true,
            invokeBuiltinsThroughBroker: false,
            productSkillBundle: productSkillBundle,
            selectedExternalMCPIDs: selectedExternalMCPIDs,
            preactivatedSkillRefs: preactivatedSkillRefs,
            restoredCapabilityDisclosures: restoredCapabilityDisclosures
        )
    }

    func taskExecutionConversationRoot(
        ownerUserID: String,
        conversationID: String
    ) throws -> URL {
        guard state.user?.id == ownerUserID else {
            throw NativePluginRuntimeError.invalidRequest("任务与当前账户不匹配")
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

    static func validateFrozenSkillPlan(
        allowedCapabilities: Set<LocalAgentTodoBuiltinCapability>,
        bundle: LocalAgentSkillBundleSnapshot
    ) throws {
        let names = authorizedVisibleToolNames(allowedCapabilities: allowedCapabilities)
        for name in names.sorted() {
            guard let authorization = LocalAgentTodoAuthorizationCatalog
                .taskToolSkillAuthorization(for: name),
                  let frozenBinding = bundle.toolSkillBindings.first(where: {
                      $0.providerID == authorization.binding.providerID
                          && $0.bindingID == authorization.binding.id
                          && $0.toolNames.contains(name)
                  }) else {
                throw NativePluginRuntimeError.invalidRequest(
                    "任务原生工具 \(name) 已授权，但冻结执行计划缺少对应 Product Skill 绑定"
                )
            }
            let expectedRefs = Set(authorization.binding.requiredSkillNames.map {
                "product-skill:\($0)"
            })
            let frozenRefs = Set([
                frozenBinding.routerSkillRef, frozenBinding.specialistSkillRef,
            ])
            guard frozenRefs == expectedRefs,
                  expectedRefs.allSatisfy({ skillRef in
                      bundle.documents.contains { $0.skillRef == skillRef }
                  }) else {
                throw NativePluginRuntimeError.invalidRequest(
                    "任务原生工具 \(name) 的 Product Skill 冻结闭包不完整"
                )
            }
        }
    }

    func definitions() async throws -> [AgentToolDefinition] {
        let taskTools = NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools
        let expectedNames = Self.authorizedVisibleToolNames(
            allowedCapabilities: allowedCapabilities
        )
        let definitions = try taskTools.compactMap { value -> AgentToolDefinition? in
            guard case let .object(tool) = value,
                  case let .string(name)? = tool["name"],
                  case let .string(description)? = tool["description"],
                  let schema = tool["parameters"] else { return nil }
            guard expectedNames.contains(name) else { return nil }
            guard let authorization = LocalAgentTodoAuthorizationCatalog
                .taskToolSkillAuthorization(for: name),
                  allowedCapabilities.contains(authorization.capability) else {
                throw NativePluginRuntimeError.invalidRequest(
                    "任务原生工具 \(name) 已授权，但缺少唯一的 Product Skill 绑定"
                )
            }
            return .init(
                name: name,
                description: description,
                schema: try JSONEncoder().encode(schema),
                effect: authorization.effect,
                providerID: authorization.binding.providerID,
                skillBindingID: authorization.binding.id
            )
        }
        guard Set(definitions.map(\.name)) == expectedNames else {
            throw NativePluginRuntimeError.invalidRequest(
                "任务原生工具授权与 Product Skill 冻结计划不完整"
            )
        }
        return definitions
    }

    private static func authorizedVisibleToolNames(
        allowedCapabilities: Set<LocalAgentTodoBuiltinCapability>
    ) -> Set<String> {
        let visibleNames = Set(
            NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools.compactMap {
                value -> String? in
                guard case let .object(tool) = value,
                      case let .string(name)? = tool["name"] else { return nil }
                return name
            }
        )
        return Set(allowedCapabilities.flatMap {
            LocalAgentTodoAuthorizationCatalog.descriptor(for: $0).toolNames
        }).intersection(visibleNames)
    }

    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        .failure("请通过 Task 原生工具直接调用 `\(call.name)`。")
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
