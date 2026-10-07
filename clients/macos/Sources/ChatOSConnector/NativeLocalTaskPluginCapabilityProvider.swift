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
    func makeTaskExecutionCapabilityToolProvider(
        ownerUserID: String,
        runID: String,
        conversationID: String,
        projectContext: LocalConnectorPluginApplicationContext,
        executionRootURL: URL,
        workspaceScopeID: String,
        pluginIDs: [String]?
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
        if let pluginIDs {
            let installedByKey = Dictionary(
                uniqueKeysWithValues: installedPlugins.map { ($0.pluginKey, $0) }
            )
            selectedPlugins = try Array(Set(pluginIDs)).sorted().map { pluginKey in
                guard let plugin = installedByKey[pluginKey] else {
                    throw NativePluginRuntimeError.invalidRequest(
                        "本地任务选择的 Plugin 未安装或已停用：\(pluginKey)"
                    )
                }
                return plugin
            }
        } else {
            selectedPlugins = installedPlugins
        }
        return NativeAgentCapabilityToolProvider(
            service: self,
            ownerUserID: ownerUserID,
            runContext: runContext,
            projectContext: projectContext,
            resolvedProject: resolvedProject,
            fallbackWorkspaceID: workspaceScopeID,
            fallbackWorkspaceRootURL: executionRootURL,
            builtinCapabilities: [],
            installedPlugins: selectedPlugins,
            executionPresentation: .localTaskExecution,
            fallbackToSelectedOptions: true
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
