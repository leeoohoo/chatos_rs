import ChatOSAgentRuntime
import ChatOSCore
import Foundation

public enum LocalAgentBuilderError: LocalizedError, Sendable, Equatable {
    case invalidBrief
    case roomUnavailable
    case noAvailableModel
    case modelUnavailable
    case pluginUnavailable(String)
    case draftNotProduced(String)

    public var errorDescription: String? {
        switch self {
        case .invalidBrief:
            "请说明希望 Agent 承担什么工作。"
        case .roomUnavailable:
            "请先创建项目 Agent 群聊。"
        case .noAvailableModel:
            "没有可供 Agent Builder 使用的模型配置。"
        case .modelUnavailable:
            "所选模型已停用、缺少密钥或不再可用。"
        case let .pluginUnavailable(pluginID):
            "草案请求的本机 Plugin 不可用：\(pluginID)"
        case let .draftNotProduced(reason):
            "Agent Builder 没有生成可确认的草案：\(reason)"
        }
    }
}

/// Runs the built-in Builder locally. The model can inspect only frozen project/catalog snapshots
/// and can only finish by returning a `LocalAgentDraft`; creation remains a separate host action.
public struct LocalAgentBuilderService: Sendable {
    private let groupChatService: NativeAgentGroupChatService
    private let projectsService: NativeLocalProjectsService
    private let connectorService: NativeLocalConnectorService
    private let agentServices: any AgentServiceProviding
    private let settings: AgentSettingsStore
    private let runtime: AgentRuntime

    public init(
        groupChatService: NativeAgentGroupChatService,
        projectsService: NativeLocalProjectsService,
        connectorService: NativeLocalConnectorService,
        agentServices: any AgentServiceProviding,
        settings: AgentSettingsStore = .init(),
        runtime: AgentRuntime = .init()
    ) {
        self.groupChatService = groupChatService
        self.projectsService = projectsService
        self.connectorService = connectorService
        self.agentServices = agentServices
        self.settings = settings
        self.runtime = runtime
    }

    public func loadResources(ownerUserID: String) async throws -> LocalAgentBuilderResources {
        let catalog = try await connectorService.fetchModelCatalog(refresh: false)
        let models = catalog.items.compactMap { model -> LocalAgentBuilderModelOption? in
            guard model.enabled, model.taskEnabled, model.hasAPIKey else { return nil }
            return .init(
                id: model.id,
                name: model.name,
                provider: model.provider,
                modelName: model.modelName
            )
        }.sorted {
            if $0.name != $1.name {
                return $0.name.localizedStandardCompare($1.name) == .orderedAscending
            }
            return $0.id < $1.id
        }
        let plugins = try await connectorService.installedAgentPlugins(ownerUserID: ownerUserID)
            .map {
                LocalAgentBuilderPluginOption(
                    id: $0.id,
                    name: $0.displayName,
                    description: $0.description
                )
            }
        return .init(models: models, plugins: plugins)
    }

    public func generateDraft(
        ownerUserID: String,
        projectID: String,
        brief: String,
        builderModelConfigID: String
    ) async throws -> LocalAgentDraft {
        let trimmedBrief = brief.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedBrief.isEmpty, trimmedBrief.count <= 8_000, !trimmedBrief.contains("\0") else {
            throw LocalAgentBuilderError.invalidBrief
        }
        let store = try await groupChatService.store()
        guard let room = try await store.activeRoom(ownerUserID: ownerUserID, projectID: projectID) else {
            throw LocalAgentBuilderError.roomUnavailable
        }
        let projectRegistry = try await projectsService.registry()
        guard let project = try await projectRegistry.get(ownerUserID: ownerUserID, id: projectID),
              project.status == .active else {
            throw ProjectRegistryError.notFound
        }
        let resources = try await loadResources(ownerUserID: ownerUserID)
        guard !resources.models.isEmpty else { throw LocalAgentBuilderError.noAvailableModel }
        guard resources.models.contains(where: { $0.id == builderModelConfigID }) else {
            throw LocalAgentBuilderError.modelUnavailable
        }
        let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
        let profiles = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: false)
        let profilesByID = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0) })
        let snapshot = LocalAgentBuilderProjectSnapshot(
            projectID: project.id,
            projectName: project.draft.name,
            projectDescription: project.draft.description,
            roomName: room.draft.name,
            roomGoal: room.draft.goal,
            members: members.map {
                .init(
                    name: profilesByID[$0.agentID]?.draft.name ?? $0.agentID,
                    role: $0.draft.role,
                    responsibility: $0.draft.responsibility
                )
            }
        )
        let provider = try LocalAgentBuilderToolProvider(
            project: snapshot,
            models: resources.models,
            plugins: resources.plugins
        )
        let registry = try await AgentToolProviderRegistry(providers: [provider])
        let runID = UUID()
        let scope = "account:\(ownerUserID):project:\(projectID):agent-builder:\(runID.uuidString.lowercased())"
        var checkpoint = AgentRunCheckpoint(
            scope: scope,
            messages: Self.initialMessages(brief: trimmedBrief)
        )
        checkpoint.id = runID

        var memoryProvider: AgentMemoryContextProvider?
        do {
            let memoryScope = try AgentMemoryScope(
                tenantID: ownerUserID,
                agentID: "builtin-agent-builder",
                projectID: projectID,
                runID: runID,
                runtimeScope: scope
            )
            let memory = try await agentServices.makeAgentMemory(scope: memoryScope)
            try await memory.ensureThread()
            let candidate = AgentMemoryContextProvider(scope: memoryScope, service: memory)
            checkpoint = try candidate.bind(checkpoint)
            checkpoint.memory?.threadCreated = true
            memoryProvider = candidate
        } catch {
            // A fresh Builder run can still produce a draft while Memory Engine is offline.
        }

        var policy = try settings.load().global
        policy.maximumModelCalls = min(policy.maximumModelCalls, 12)
        policy.runTimeoutSeconds = min(policy.runTimeoutSeconds, 900)
        policy.maximumNoProgressRounds = min(policy.maximumNoProgressRounds, 4)
        try policy.validate()
        let model = try await agentServices.makeAgentModel(
            configID: builderModelConfigID,
            policy: policy
        )
        let result = try await runtime.run(
            checkpoint: checkpoint,
            scope: scope,
            policy: policy,
            model: model,
            tools: registry.definitions,
            execute: { call in try await registry.execute(call) },
            contextProvider: memoryProvider
        )
        guard result.status == .completed, let draft = await provider.currentDraft() else {
            throw LocalAgentBuilderError.draftNotProduced(
                result.stopReason ?? "运行在完成草案前停止"
            )
        }
        try validate(draft: draft, resources: resources)
        return draft
    }

    /// This is the only creation path used by a Builder proposal. It re-reads the current local
    /// allowlists at confirmation time so a stale draft cannot retain a removed model or Plugin.
    public func createConfirmedDraft(
        ownerUserID: String,
        projectID: String,
        draft: LocalAgentDraft
    ) async throws -> LocalAgentProfile {
        try draft.validate()
        let resources = try await loadResources(ownerUserID: ownerUserID)
        try validate(draft: draft, resources: resources)
        let store = try await groupChatService.store()
        guard let room = try await store.activeRoom(ownerUserID: ownerUserID, projectID: projectID) else {
            throw LocalAgentBuilderError.roomUnavailable
        }
        let agent = try await store.createAgent(ownerUserID: ownerUserID, draft: draft.profileDraft)
        _ = try await store.addMember(
            ownerUserID: ownerUserID,
            roomID: room.id,
            agentID: agent.id,
            draft: draft.memberDraft
        )
        let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
        if room.defaultAgentID == nil, members.count == 1 {
            _ = try await store.setDefaultAgent(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agent.id
            )
        }
        return agent
    }

    /// Confirms a proposal submitted by a room Agent. Live model and Plugin allowlists are checked
    /// before the store atomically creates the profile, joins it to the room and resolves the
    /// proposal, so a stale or partially retried confirmation cannot create duplicate members.
    public func approveProposal(
        ownerUserID: String,
        projectID: String,
        proposal: LocalAgentCreationProposal
    ) async throws -> LocalAgentProposalApproval {
        guard proposal.ownerUserID == ownerUserID, proposal.status == .pending else {
            throw AgentGroupChatError.conflict
        }
        let resources = try await loadResources(ownerUserID: ownerUserID)
        try validate(draft: proposal.draft, resources: resources)
        let store = try await groupChatService.store()
        guard let room = try await store.activeRoom(ownerUserID: ownerUserID, projectID: projectID),
              room.id == proposal.roomID else {
            throw LocalAgentBuilderError.roomUnavailable
        }
        return try await store.approveAgentProposal(
            ownerUserID: ownerUserID,
            roomID: room.id,
            proposalID: proposal.id,
            nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
        )
    }

    private func validate(
        draft: LocalAgentDraft,
        resources: LocalAgentBuilderResources
    ) throws {
        try draft.validate()
        guard resources.models.contains(where: { $0.id == draft.modelConfigID }) else {
            throw LocalAgentBuilderError.modelUnavailable
        }
        let installedPluginIDs = Set(resources.plugins.map(\.id))
        if let unavailable = draft.pluginIDs.first(where: { !installedPluginIDs.contains($0) }) {
            throw LocalAgentBuilderError.pluginUnavailable(unavailable)
        }
    }

    private static func initialMessages(brief: String) -> [AgentMessage] {
        let system = """
        你是 ChatOS 客户端内置的 Agent Builder。你的唯一任务是为当前项目群聊设计一个普通 Agent 草案。
        先调用 project_inspect、model_list 和 plugin_list_installed 获取客户端提供的受控快照，然后单独调用 agent_draft 提交草案。只能选择 model_list 和 plugin_list_installed 返回的 id。不要创建公司、组织或账号；不要假设未提供的权限；不要把 Agent Builder、创建 Agent 或管理成员的能力写入普通 Agent。角色 Prompt 要明确职责、边界、如何使用项目群聊和已授权 Plugin。
        agent_draft 只会生成等待用户确认的结构化草案，不会创建 Agent。
        """
        return [
            .init(role: .system, content: system),
            .init(role: .user, content: "用户希望创建这样的项目 Agent：\n\(brief)"),
        ]
    }
}

struct LocalAgentBuilderProjectSnapshot: Codable, Sendable, Equatable {
    struct Member: Codable, Sendable, Equatable {
        let name: String
        let role: String
        let responsibility: String
    }

    let projectID: String
    let projectName: String
    let projectDescription: String
    let roomName: String
    let roomGoal: String
    let members: [Member]
}

actor LocalAgentBuilderToolProvider: AgentToolProvider {
    static let projectInspectToolName = "project_inspect"
    static let modelListToolName = "model_list"
    static let pluginListToolName = "plugin_list_installed"
    static let agentDraftToolName = "agent_draft"

    private let project: LocalAgentBuilderProjectSnapshot
    private let models: [LocalAgentBuilderModelOption]
    private let plugins: [LocalAgentBuilderPluginOption]
    private var draft: LocalAgentDraft?

    init(
        project: LocalAgentBuilderProjectSnapshot,
        models: [LocalAgentBuilderModelOption],
        plugins: [LocalAgentBuilderPluginOption]
    ) throws {
        guard !models.isEmpty,
              Set(models.map(\.id)).count == models.count,
              Set(plugins.map(\.id)).count == plugins.count else {
            throw LocalAgentBuilderError.noAvailableModel
        }
        self.project = project
        self.models = models
        self.plugins = plugins
    }

    func currentDraft() -> LocalAgentDraft? { draft }

    func definitions() async throws -> [AgentToolDefinition] {
        [
            .init(
                name: Self.projectInspectToolName,
                description: "读取当前本地项目、群聊目标和现有 Agent 职责的冻结快照。",
                schema: Self.emptyObjectSchema
            ),
            .init(
                name: Self.modelListToolName,
                description: "列出当前可用于普通 Agent 的已启用模型配置。",
                schema: Self.emptyObjectSchema
            ),
            .init(
                name: Self.pluginListToolName,
                description: "列出本机已经安装、启用并可供 Agent 使用的 Plugin。",
                schema: Self.emptyObjectSchema
            ),
            .init(
                name: Self.agentDraftToolName,
                description: "提交一个等待用户确认的 Agent 草案；这不会创建 Agent 或修改群聊。",
                schema: try Self.draftSchema(models: models, plugins: plugins),
                effect: .terminal
            ),
        ]
    }

    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        switch call.name {
        case Self.projectInspectToolName:
            try Self.requireEmptyArguments(call)
            return try Self.outcome(project)
        case Self.modelListToolName:
            try Self.requireEmptyArguments(call)
            return try Self.outcome(models)
        case Self.pluginListToolName:
            try Self.requireEmptyArguments(call)
            return try Self.outcome(plugins)
        case Self.agentDraftToolName:
            let proposed = try JSONDecoder().decode(
                LocalAgentDraft.self,
                from: Data(call.arguments.utf8)
            )
            try proposed.validate()
            guard models.contains(where: { $0.id == proposed.modelConfigID }) else {
                return .failure(LocalAgentBuilderError.modelUnavailable.localizedDescription)
            }
            let installed = Set(plugins.map(\.id))
            if let unavailable = proposed.pluginIDs.first(where: { !installed.contains($0) }) {
                return .failure(LocalAgentBuilderError.pluginUnavailable(unavailable).localizedDescription)
            }
            draft = proposed
            return try Self.outcome(proposed)
        default:
            return .failure("Agent Builder 工具不可用：\(call.name)")
        }
    }

    private static let emptyObjectSchema = Data(
        #"{"type":"object","properties":{},"additionalProperties":false}"#.utf8
    )

    private static func draftSchema(
        models: [LocalAgentBuilderModelOption],
        plugins: [LocalAgentBuilderPluginOption]
    ) throws -> Data {
        let schema: [String: Any] = [
            "type": "object",
            "properties": [
                "name": ["type": "string", "minLength": 1, "maxLength": 120],
                "role": ["type": "string", "minLength": 1, "maxLength": 160],
                "responsibility": ["type": "string", "maxLength": 8_000],
                "rolePrompt": ["type": "string", "minLength": 1, "maxLength": 32_000],
                "modelConfigID": ["type": "string", "enum": models.map(\.id)],
                "pluginIDs": [
                    "type": "array",
                    "items": ["type": "string", "enum": plugins.map(\.id)],
                    "maxItems": min(plugins.count, 100),
                    "uniqueItems": true,
                ],
                "rationale": ["type": "string", "maxLength": 4_000],
            ],
            "required": [
                "name", "role", "responsibility", "rolePrompt",
                "modelConfigID", "pluginIDs", "rationale",
            ],
            "additionalProperties": false,
        ]
        return try JSONSerialization.data(withJSONObject: schema, options: [.sortedKeys])
    }

    private static func requireEmptyArguments(_ call: AgentToolCall) throws {
        guard let value = try? JSONSerialization.jsonObject(with: Data(call.arguments.utf8)),
              let object = value as? [String: Any], object.isEmpty else {
            throw AgentGroupChatError.invalidField("arguments")
        }
    }

    private static func outcome<Value: Encodable>(_ value: Value) throws -> AgentToolOutcome {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return .init(String(decoding: try encoder.encode(value), as: UTF8.self))
    }
}
