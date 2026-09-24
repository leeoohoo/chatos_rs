import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// One identity-bound session on the local Relay MCP. Tool arguments can never select another
/// account, project, room, Agent, delivery or Memory identity.
public struct LocalAgentChatToolProvider: AgentToolProvider, Sendable {
    public static let bootstrapToolName = "relay_bootstrap"
    public static let workspaceSnapshotToolName = "agent_workspace_snapshot"
    public static let getTriggerToolName = "chat_get_trigger"
    public static let listMembersToolName = "chat_list_members"
    public static let readUnreadToolName = "chat_read_unread"
    public static let readAllUnreadToolName = "chat_read_all_unread"
    public static let inboxSendToolName = "chat_inbox_send"
    public static let readMessagesToolName = "chat_read_messages"
    public static let readAttachmentToolName = "chat_read_attachment"
    public static let createDocumentToolName = "chat_document_create"
    public static let markReadToolName = "chat_mark_read"
    public static let openDirectToolName = "chat_direct_open"
    public static let sendDirectToolName = "chat_direct_send"
    public static let sendTeamToolName = "chat_team_send"
    public static let proposeMemberToolName = "agent_propose_member"
    public static let proposeExistingMemberToolName = "agent_propose_existing_member"
    public static let proposeMemberRemovalToolName = "agent_propose_member_removal"
    public static let sendMessageToolName = "chat_send_message"
    public static let completeHeartbeatToolName = "chat_heartbeat_complete"
    public static let completeManagerCycleToolName = "agent_cycle_complete"
    public static let todoListToolName = "todo_list"
    public static let todoScheduleStateToolName = "todo_schedule_state"
    public static let todoStartNextToolName = "todo_start_next"
    public static let todoAddToolName = "todo_add"
    public static let todoUpdateToolName = "todo_update"
    public static let todoReorderToolName = "todo_reorder"
    public static let todoExecutionOptionsToolName = "todo_execution_options"
    public static let todoDependencyOptionsToolName = "todo_dependency_options"
    public static let todoGetContextToolName = "todo_get_context"
    public static let todoProgressAppendToolName = "todo_progress_append"
    public static let todoReadProgressToolName = "todo_read_progress"
    public static let todoCompleteToolName = "todo_complete"
    public static let todoBlockToolName = "todo_block"
    public static let teamAssetListToolName = "team_asset_list"
    public static let teamAssetGetToolName = "team_asset_get"
    public static let teamAssetCreateToolName = "team_asset_create"
    public static let teamAssetUpdateToolName = "team_asset_update"
    public static let teamAssetArchiveToolName = "team_asset_archive"
    public static let projectDashboardGetToolName = "project_dashboard_get"
    public static let projectDashboardUpdateToolName = "project_dashboard_update"
    public static let agentSkillActivateToolName = "agent_skill_activate"
    public static let agentSkillListResourcesToolName = "agent_skill_list_resources"
    public static let agentSkillReadResourceToolName = "agent_skill_read_resource"
    let store: any AgentGroupChatStore
    let context: LocalAgentChatRunContext
    let professions: [LocalAgentProfessionDefinition]
    let limits: AgentGroupChatRoutingLimits
    let now: @Sendable () -> Int64
    let references: LocalAgentRunReferenceVault
    let todoPluginOptions: [LocalAgentTodoPluginOption]
    let todoCancellationHandler: @Sendable (String) async -> Void
    let roomChangeHandler: @Sendable (String) async -> Void
    let progressiveSkills: LocalAgentProgressiveSkillSession
    let productSkills: ProductToolSkillSession

    public init(
        store: any AgentGroupChatStore,
        context: LocalAgentChatRunContext,
        professions: [LocalAgentProfessionDefinition] = LocalAgentSkillCatalog.professions,
        progressiveSkillSnapshot: LocalAgentProgressiveSkillSnapshot? = nil,
        productSkillSession: ProductToolSkillSession = .init(),
        todoPluginOptions: [LocalAgentTodoPluginOption] = [],
        limits: AgentGroupChatRoutingLimits = .init(),
        todoCancellationHandler: @escaping @Sendable (String) async -> Void = { _ in },
        roomChangeHandler: @escaping @Sendable (String) async -> Void = { _ in },
        documentDraftDirectoryURL: URL,
        now: @escaping @Sendable () -> Int64 = {
            Int64(Date().timeIntervalSince1970 * 1_000)
        }
    ) throws {
        try limits.validate()
        self.store = store
        self.context = context
        self.professions = professions
        self.progressiveSkills = LocalAgentProgressiveSkillSession(
            snapshot: progressiveSkillSnapshot
        )
        self.productSkills = productSkillSession
        self.limits = limits
        self.now = now
        self.references = LocalAgentRunReferenceVault(
            documentDraftDirectoryURL: documentDraftDirectoryURL,
            runContext: context,
            pluginOptions: todoPluginOptions
        )
        self.todoPluginOptions = todoPluginOptions
        self.todoCancellationHandler = todoCancellationHandler
        self.roomChangeHandler = roomChangeHandler
    }

    public func definitions() async throws -> [AgentToolDefinition] {
        var definitions = Self.toolDefinitions
        if !(await progressiveSkills.hasSkills) {
            let skillTools = Set([
                Self.agentSkillActivateToolName,
                Self.agentSkillListResourcesToolName,
                Self.agentSkillReadResourceToolName,
            ])
            definitions.removeAll { skillTools.contains($0.name) }
        }
        guard let delivery = try await store.delivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ) else { throw AgentGroupChatError.notFound }
        if delivery.lane == .executor {
            let executorTools: Set<String> = [
                Self.todoGetContextToolName,
                Self.todoProgressAppendToolName,
                Self.todoCompleteToolName,
                Self.todoBlockToolName,
                Self.teamAssetListToolName,
                Self.teamAssetGetToolName,
                Self.agentSkillActivateToolName,
                Self.agentSkillListResourcesToolName,
                Self.agentSkillReadResourceToolName,
            ]
            return try await skillBoundDefinitions(
                definitions.filter { executorTools.contains($0.name) }
            )
        }
        let executorOnly: Set<String> = [
            Self.todoGetContextToolName,
            Self.todoProgressAppendToolName,
            Self.todoCompleteToolName,
            Self.todoBlockToolName,
        ]
        definitions.removeAll { executorOnly.contains($0.name) }
        if delivery.triggerKind != .heartbeat {
            definitions.removeAll { $0.name == Self.completeHeartbeatToolName }
        }
        if !(try await canManageStaff()) {
            definitions = definitions.filter {
                $0.name != Self.proposeMemberToolName
                    && $0.name != Self.proposeExistingMemberToolName
                    && $0.name != Self.proposeMemberRemovalToolName
            }
        } else {
            definitions.removeAll { $0.name == Self.proposeMemberToolName }
            definitions.append(try memberProposalDefinition())
            if try await store.room(
                ownerUserID: context.ownerUserID,
                roomID: context.roomID
            )?.conversationKind.isDirect == true {
                definitions = definitions.filter { $0.name != Self.proposeMemberRemovalToolName }
            }
        }
        if !(try await managesAnyProjectTeam()) {
            let projectManagerOnly: Set<String> = [
                Self.todoAddToolName,
                Self.todoUpdateToolName,
                Self.todoReorderToolName,
                Self.todoExecutionOptionsToolName,
                Self.todoDependencyOptionsToolName,
                Self.teamAssetCreateToolName,
                Self.teamAssetUpdateToolName,
                Self.teamAssetArchiveToolName,
                Self.projectDashboardUpdateToolName,
            ]
            definitions.removeAll { projectManagerOnly.contains($0.name) }
        }
        return try await skillBoundDefinitions(definitions)
    }

    public func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        switch call.name {
        case Self.bootstrapToolName:
            return try await bootstrap(call)
        case Self.workspaceSnapshotToolName:
            return try await workspaceSnapshot(call)
        case Self.getTriggerToolName:
            return try await getTrigger(call)
        case Self.listMembersToolName:
            return try await listMembers(call)
        case Self.readUnreadToolName:
            return try await readUnread(call)
        case Self.readAllUnreadToolName:
            return try await readAllUnread(call)
        case Self.inboxSendToolName:
            return try await sendInboxMessage(call)
        case Self.readMessagesToolName:
            return try await readMessages(call)
        case Self.readAttachmentToolName:
            return try await readAttachment(call)
        case Self.createDocumentToolName:
            return try await createDocument(call)
        case Self.markReadToolName:
            return try await markRead(call)
        case Self.openDirectToolName:
            return try await openDirect(call)
        case Self.sendDirectToolName:
            return try await sendDirect(call)
        case Self.sendTeamToolName:
            return try await sendTeam(call)
        case Self.proposeMemberToolName:
            return try await proposeMember(call)
        case Self.proposeExistingMemberToolName:
            return try await proposeExistingMember(call)
        case Self.proposeMemberRemovalToolName:
            return try await proposeMemberRemoval(call)
        case Self.sendMessageToolName:
            return try await sendMessage(call)
        case Self.completeHeartbeatToolName:
            return try await completeHeartbeat(call)
        case Self.completeManagerCycleToolName:
            return try await completeManagerCycle(call)
        case Self.todoListToolName:
            return try await listTodos(call)
        case Self.todoScheduleStateToolName:
            return try await todoScheduleState(call)
        case Self.todoStartNextToolName:
            return try await startNextTodo(call)
        case Self.todoAddToolName:
            return try await addTodo(call)
        case Self.todoUpdateToolName:
            return try await updateTodo(call)
        case Self.todoReorderToolName:
            return try await reorderTodos(call)
        case Self.todoExecutionOptionsToolName:
            return try await todoExecutionOptions(call)
        case Self.todoDependencyOptionsToolName:
            return try await todoDependencyOptions(call)
        case Self.todoGetContextToolName:
            return try await todoGetContext(call)
        case Self.todoProgressAppendToolName:
            return try await appendTodoProgress(call)
        case Self.todoReadProgressToolName:
            return try await readTodoProgress(call)
        case Self.todoCompleteToolName:
            return try await completeTodo(call)
        case Self.todoBlockToolName:
            return try await blockTodo(call)
        case Self.teamAssetListToolName:
            return try await listTeamAssets(call)
        case Self.teamAssetGetToolName:
            return try await getTeamAsset(call)
        case Self.teamAssetCreateToolName:
            return try await createTeamAsset(call)
        case Self.teamAssetUpdateToolName:
            return try await updateTeamAsset(call)
        case Self.teamAssetArchiveToolName:
            return try await archiveTeamAsset(call)
        case Self.projectDashboardGetToolName:
            return try await getProjectDashboard(call)
        case Self.projectDashboardUpdateToolName:
            return try await updateProjectDashboard(call)
        case Self.agentSkillActivateToolName:
            return try await activateAgentSkill(call)
        case Self.agentSkillListResourcesToolName:
            return try await listAgentSkillResources(call)
        case Self.agentSkillReadResourceToolName:
            return try await readAgentSkillResource(call)
        default:
            return .failure("群聊工具不可用：\(call.name)")
        }
    }



    private func memberProposalDefinition() throws -> AgentToolDefinition {
        let schema: [String: Any] = [
            "type": "object",
            "properties": [
                "name": ["type": "string", "minLength": 1, "maxLength": 120],
                "role": ["type": "string", "minLength": 1, "maxLength": 160],
                "responsibility": ["type": "string", "maxLength": 8_000],
                "role_prompt": ["type": "string", "minLength": 1, "maxLength": 32_000],
                "thinking_level": [
                    "type": "string",
                    "enum": LocalAgentThinkingLevelCatalog.allValues.sorted(),
                    "description": "省略时继承当前 Agent 的思考等级；确认创建时会按实际模型能力重新校验。",
                ],
                "profession_key": [
                    "type": "string",
                    "enum": professions.map(\.key),
                    "description": professions.map { "\($0.key)=\($0.label)" }.joined(separator: "；"),
                ],
                "rationale": ["type": "string", "maxLength": 4_000],
            ],
            "required": ["name", "role", "role_prompt", "profession_key"],
            "additionalProperties": false,
        ]
        return .init(
            name: Self.proposeMemberToolName,
            description: "使用已授予的人员管理权限，向 Human 提交一个新 Agent 草案。必须从客户端目录选择职业；模型配置由客户端继承并透传，AI 不填写模型 ID；thinking_level 省略时继承当前 Agent。该工具只持久化待确认提案，绝不会直接创建 Agent。",
            schema: try JSONSerialization.data(withJSONObject: schema, options: [.sortedKeys]),
            effect: .write
        )
    }

    private func skillBoundDefinitions(
        _ definitions: [AgentToolDefinition]
    ) async throws -> [AgentToolDefinition] {
        var result: [AgentToolDefinition] = []
        var registeredBindingIDs = Set<String>()
        for var definition in definitions {
            guard let bindingID = Self.skillBindingID(for: definition.name) else {
                result.append(definition)
                continue
            }
            definition.providerID = ProductToolProviderID.localAgentChat
            definition.skillBindingID = bindingID
            result.append(definition)
            registeredBindingIDs.insert(bindingID)
        }
        for bindingID in registeredBindingIDs.sorted() {
            try await productSkills.register(
                providerID: ProductToolProviderID.localAgentChat,
                skillBindingID: bindingID
            )
        }
        return result
    }

    private static func skillBindingID(for toolName: String) -> String? {
        switch toolName {
        case agentSkillActivateToolName, agentSkillListResourcesToolName,
             agentSkillReadResourceToolName:
            ProductToolSkillBindingID.agentSkillControlPlane
        case bootstrapToolName, workspaceSnapshotToolName, getTriggerToolName,
             listMembersToolName, readUnreadToolName, readAllUnreadToolName,
             readMessagesToolName, readAttachmentToolName:
            ProductToolSkillBindingID.relayContext
        case inboxSendToolName, createDocumentToolName, markReadToolName,
             openDirectToolName, sendDirectToolName, sendTeamToolName,
             sendMessageToolName, completeHeartbeatToolName, completeManagerCycleToolName:
            ProductToolSkillBindingID.collaborationMessaging
        case proposeMemberToolName, proposeExistingMemberToolName,
             proposeMemberRemovalToolName:
            ProductToolSkillBindingID.agentStaffing
        case todoListToolName, todoScheduleStateToolName, todoStartNextToolName,
             todoAddToolName, todoUpdateToolName, todoReorderToolName,
             todoExecutionOptionsToolName, todoDependencyOptionsToolName:
            ProductToolSkillBindingID.todoPlanning
        case todoGetContextToolName, todoProgressAppendToolName, todoReadProgressToolName,
             todoCompleteToolName, todoBlockToolName:
            ProductToolSkillBindingID.todoExecution
        case teamAssetListToolName, teamAssetGetToolName, teamAssetCreateToolName,
             teamAssetUpdateToolName, teamAssetArchiveToolName:
            ProductToolSkillBindingID.teamKnowledge
        case projectDashboardGetToolName, projectDashboardUpdateToolName:
            ProductToolSkillBindingID.projectDashboard
        default:
            nil
        }
    }

    public static func skillCoverageReport() -> ToolSkillCoverageReport {
        ToolSkillCoverageCatalog.product.audit(Self.toolDefinitions.map {
            .init(
                providerID: ProductToolProviderID.localAgentChat,
                toolName: $0.name,
                skillBindingID: skillBindingID(for: $0.name)
            )
        })
    }
}
