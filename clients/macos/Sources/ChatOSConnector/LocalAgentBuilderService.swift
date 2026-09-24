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
    private let skillLibrary: LocalAgentSkillLibrary?

    public init(
        groupChatService: NativeAgentGroupChatService,
        projectsService: NativeLocalProjectsService,
        connectorService: NativeLocalConnectorService,
        agentServices: any AgentServiceProviding,
        skillLibrary: LocalAgentSkillLibrary? = nil,
        settings: AgentSettingsStore = .init(),
        runtime: AgentRuntime = .init()
    ) {
        self.groupChatService = groupChatService
        self.projectsService = projectsService
        self.connectorService = connectorService
        self.agentServices = agentServices
        self.skillLibrary = skillLibrary
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
                modelName: model.modelName,
                supportsReasoning: model.supportsReasoning,
                defaultThinkingLevel: model.taskThinkingLevel,
                thinkingLevels: model.supportsReasoning
                    ? LocalAgentThinkingLevelCatalog.values(provider: model.provider)
                    : []
            )
        }.sorted {
            if $0.name != $1.name {
                return $0.name.localizedStandardCompare($1.name) == .orderedAscending
            }
            return $0.id < $1.id
        }
        return .init(models: models, plugins: [])
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
            projectName: project.draft.name,
            projectDescription: project.draft.description,
            projectTypeKey: project.draft.projectTypeKey,
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
            professions: skillLibrary?.professions(ownerUserID: ownerUserID)
                ?? LocalAgentSkillCatalog.professions
        )
        let registry = try await AgentToolProviderRegistry(providers: [provider])
        let runID = UUID()
        let scope = "account:\(ownerUserID):project:\(projectID):agent-builder:\(runID.uuidString.lowercased())"
        var checkpoint = AgentRunCheckpoint(
            scope: scope,
            messages: try Self.initialMessages(brief: trimmedBrief)
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

    /// Confirms a proposal submitted by a room Agent. The live model is checked before the store
    /// atomically creates the profile, joins it to the room and resolves the proposal. Plugin
    /// access is deliberately absent from the profile and is discovered lazily at run time.
    public func approveProposal(
        ownerUserID: String,
        projectID: String,
        proposal: LocalAgentCreationProposal
    ) async throws -> LocalAgentProposalApproval {
        guard proposal.ownerUserID == ownerUserID, proposal.status == .pending else {
            throw AgentGroupChatError.conflict
        }
        let store = try await groupChatService.store()
        let resolvedDraft = try await resolvedProposalDraft(proposal, store: store)
        let resources = try await loadResources(ownerUserID: ownerUserID)
        try validate(draft: resolvedDraft, resources: resources)
        guard let room = try await store.activeRoom(ownerUserID: ownerUserID, projectID: projectID),
              room.id == proposal.roomID else {
            throw LocalAgentBuilderError.roomUnavailable
        }
        return try await store.approveAgentProposal(
            ownerUserID: ownerUserID,
            roomID: room.id,
            proposalID: proposal.id,
            nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000),
            resolvedDraft: resolvedDraft
        )
    }

    /// Confirms an Agent proposal from a direct conversation. The new profile is account-owned
    /// and remains independent; it is not silently added to the private conversation or a team.
    public func approveProposal(
        ownerUserID: String,
        roomID: String,
        proposal: LocalAgentCreationProposal
    ) async throws -> LocalAgentProposalApproval {
        guard proposal.ownerUserID == ownerUserID,
              proposal.roomID == roomID,
              proposal.status == .pending else {
            throw AgentGroupChatError.conflict
        }
        let store = try await groupChatService.store()
        let resolvedDraft = try await resolvedProposalDraft(proposal, store: store)
        let resources = try await loadResources(ownerUserID: ownerUserID)
        try validate(draft: resolvedDraft, resources: resources)
        guard let room = try await store.room(ownerUserID: ownerUserID, roomID: roomID),
              room.conversationKind.isDirect else {
            throw LocalAgentBuilderError.roomUnavailable
        }
        return try await store.approveAgentProposal(
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposal.id,
            nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000),
            resolvedDraft: resolvedDraft
        )
    }

    private func resolvedProposalDraft(
        _ proposal: LocalAgentCreationProposal,
        store: SQLiteAgentGroupChatStore
    ) async throws -> LocalAgentDraft {
        let requestedModelConfigID = proposal.draft.modelConfigID
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard Self.usesProposerModel(requestedModelConfigID) else {
            return proposal.draft
        }
        let profiles = try await store.listAgents(
            ownerUserID: proposal.ownerUserID,
            includeArchived: true
        )
        guard let proposer = profiles.first(where: { $0.id == proposal.proposerAgentID }) else {
            throw LocalAgentBuilderError.modelUnavailable
        }
        return LocalAgentDraft(
            name: proposal.draft.name,
            role: proposal.draft.role,
            responsibility: proposal.draft.responsibility,
            rolePrompt: proposal.draft.rolePrompt,
            modelConfigID: proposer.draft.modelConfigID,
            thinkingLevel: proposal.draft.thinkingLevel ?? proposer.draft.thinkingLevel,
            professionKey: proposal.draft.professionKey,
            rationale: proposal.draft.rationale
        )
    }

    static func usesProposerModel(_ modelConfigID: String) -> Bool {
        let normalized = modelConfigID.trimmingCharacters(in: .whitespacesAndNewlines)
        return ["default", "inherit", "inherit-current"].contains {
            normalized.caseInsensitiveCompare($0) == .orderedSame
        }
    }

    private func validate(
        draft: LocalAgentDraft,
        resources: LocalAgentBuilderResources
    ) throws {
        try draft.validate()
        guard let model = resources.models.first(where: { $0.id == draft.modelConfigID }) else {
            throw LocalAgentBuilderError.modelUnavailable
        }
        if let thinkingLevel = draft.thinkingLevel,
           !model.thinkingLevels.contains(thinkingLevel) {
            throw AgentGroupChatError.invalidField("thinkingLevel")
        }
    }

    private static func initialMessages(brief: String) throws -> [AgentMessage] {
        let skill = try BundledAgentSkillLoader.load(named: "chatos-agent-builder")
        return [
            .init(
                role: .system,
                content: LocalAgentPromptCatalog.render(.builderSystem)
                    + "\n\n" + skill.instructions
            ),
            .init(
                role: .user,
                content: LocalAgentPromptCatalog.render(
                    .builderUser,
                    values: ["brief": brief]
                )
            ),
        ]
    }
}

struct LocalAgentBuilderProjectSnapshot: Codable, Sendable, Equatable {
    struct Member: Codable, Sendable, Equatable {
        let name: String
        let role: String
        let responsibility: String
    }

    let projectName: String
    let projectDescription: String
    let projectTypeKey: String
    let roomName: String
    let roomGoal: String
    let members: [Member]
}

actor LocalAgentBuilderToolProvider: AgentToolProvider {
    static let projectInspectToolName = "project_inspect"
    static let modelListToolName = "model_list"
    static let professionListToolName = "profession_list"
    static let agentDraftToolName = "agent_draft"

    private let project: LocalAgentBuilderProjectSnapshot
    private let models: [LocalAgentBuilderModelOption]
    private let professions: [LocalAgentProfessionDefinition]
    private var draft: LocalAgentDraft?

    init(
        project: LocalAgentBuilderProjectSnapshot,
        models: [LocalAgentBuilderModelOption],
        professions: [LocalAgentProfessionDefinition]
    ) throws {
        guard !models.isEmpty, Set(models.map(\.id)).count == models.count else {
            throw LocalAgentBuilderError.noAvailableModel
        }
        self.project = project
        self.models = models
        self.professions = professions
    }

    func currentDraft() -> LocalAgentDraft? { draft }

    func definitions() async throws -> [AgentToolDefinition] {
        [
            .init(
                name: Self.projectInspectToolName,
                description: "读取当前本地项目、群聊目标和现有 Agent 职责的冻结快照。",
                schema: Self.emptyObjectSchema,
                providerID: ProductToolProviderID.agentBuilder,
                skillBindingID: ProductToolSkillBindingID.agentBuilder
            ),
            .init(
                name: Self.modelListToolName,
                description: "列出当前可用于普通 Agent 的已启用模型配置。",
                schema: Self.emptyObjectSchema,
                providerID: ProductToolProviderID.agentBuilder,
                skillBindingID: ProductToolSkillBindingID.agentBuilder
            ),
            .init(
                name: Self.professionListToolName,
                description: "列出 ChatOS 内置职业及稳定 key。Agent 必须选择一个职业。",
                schema: Self.emptyObjectSchema,
                providerID: ProductToolProviderID.agentBuilder,
                skillBindingID: ProductToolSkillBindingID.agentBuilder
            ),
            .init(
                name: Self.agentDraftToolName,
                description: "提交一个等待用户确认的 Agent 草案；这不会创建 Agent 或修改群聊。",
                schema: try Self.draftSchema(models: models, professions: professions),
                effect: .terminal,
                providerID: ProductToolProviderID.agentBuilder,
                skillBindingID: ProductToolSkillBindingID.agentBuilder
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
        case Self.professionListToolName:
            try Self.requireEmptyArguments(call)
            return try Self.outcome(professions.map {
                ProfessionOption(
                    key: $0.key,
                    label: $0.label,
                    category: $0.categoryLabel,
                    description: $0.description
                )
            })
        case Self.agentDraftToolName:
            let proposed = try JSONDecoder().decode(
                LocalAgentDraft.self,
                from: Data(call.arguments.utf8)
            )
            try proposed.validate()
            guard let selectedModel = models.first(where: { $0.id == proposed.modelConfigID }) else {
                return .failure(LocalAgentBuilderError.modelUnavailable.localizedDescription)
            }
            let resolvedThinkingLevel = proposed.thinkingLevel
                ?? selectedModel.defaultThinkingLevel
            if let resolvedThinkingLevel,
               !selectedModel.thinkingLevels.contains(resolvedThinkingLevel) {
                return .failure(
                    AgentGroupChatError.invalidField("thinkingLevel").localizedDescription
                )
            }
            guard professions.contains(where: { $0.key == proposed.professionKey }) else {
                return .failure("职业已不可用，请重新读取 profession_list。")
            }
            let resolved = LocalAgentDraft(
                name: proposed.name,
                role: proposed.role,
                responsibility: proposed.responsibility,
                rolePrompt: proposed.rolePrompt,
                modelConfigID: proposed.modelConfigID,
                thinkingLevel: resolvedThinkingLevel,
                professionKey: proposed.professionKey,
                rationale: proposed.rationale
            )
            draft = resolved
            return try Self.outcome(resolved)
        default:
            return .failure("Agent Builder 工具不可用：\(call.name)")
        }
    }

    private static let emptyObjectSchema = Data(
        #"{"type":"object","properties":{},"additionalProperties":false}"#.utf8
    )

    private struct ProfessionOption: Encodable {
        let key: String
        let label: String
        let category: String
        let description: String
    }

    private static func draftSchema(
        models: [LocalAgentBuilderModelOption],
        professions: [LocalAgentProfessionDefinition]
    ) throws -> Data {
        let schema: [String: Any] = [
            "type": "object",
            "properties": [
                "name": ["type": "string", "minLength": 1, "maxLength": 120],
                "role": ["type": "string", "minLength": 1, "maxLength": 160],
                "responsibility": ["type": "string", "maxLength": 8_000],
                "rolePrompt": ["type": "string", "minLength": 1, "maxLength": 32_000],
                "modelConfigID": ["type": "string", "enum": models.map(\.id)],
                "thinkingLevel": [
                    "type": "string",
                    "enum": Array(Set(models.flatMap(\.thinkingLevels))).sorted(),
                    "description": "必须是所选模型 model_list.thinkingLevels 中的值；省略时使用该模型配置的默认等级。",
                ],
                "professionKey": ["type": "string", "enum": professions.map(\.key)],
                "rationale": ["type": "string", "maxLength": 4_000],
            ],
            "required": [
                "name", "role", "responsibility", "rolePrompt",
                "modelConfigID", "professionKey", "rationale",
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
