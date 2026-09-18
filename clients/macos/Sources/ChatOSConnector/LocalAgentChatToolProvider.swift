import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// Safe catalog metadata supplied by the client. `pluginID` remains inside the provider and is
/// mapped from a run-scoped opaque reference before a Todo execution plan is persisted.
public struct LocalAgentTodoPluginOption: Sendable, Equatable {
    public let pluginID: String
    public let displayName: String
    public let description: String

    public init(pluginID: String, displayName: String, description: String) {
        self.pluginID = pluginID
        self.displayName = displayName
        self.description = description
    }
}

/// Immutable authority for one claimed local delivery. Caller-controlled tool arguments never
/// select the sender, account, project, room or Memory identity.
public struct LocalAgentChatRunContext: Codable, Sendable, Equatable {
    public let ownerUserID: String
    public let projectID: String
    public let roomID: String
    public let agentID: String
    public let deliveryID: String
    public let triggerMessageID: String
    public let rootMessageID: String
    public let runID: String
    public let hopCount: Int
    public let lane: LocalAgentRunLane

    public init(
        ownerUserID: String,
        projectID: String,
        roomID: String,
        agentID: String,
        deliveryID: String,
        triggerMessageID: String,
        rootMessageID: String,
        runID: String,
        hopCount: Int,
        lane: LocalAgentRunLane = .manager
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        try AgentGroupChatValidation.identifier(triggerMessageID, field: "triggerMessageID")
        try AgentGroupChatValidation.identifier(rootMessageID, field: "rootMessageID")
        try AgentGroupChatValidation.identifier(runID, field: "runID")
        guard (0...64).contains(hopCount) else {
            throw AgentGroupChatError.invalidField("hopCount")
        }
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.roomID = roomID
        self.agentID = agentID
        self.deliveryID = deliveryID
        self.triggerMessageID = triggerMessageID
        self.rootMessageID = rootMessageID
        self.runID = runID
        self.hopCount = hopCount
        self.lane = lane
    }

    private enum CodingKeys: String, CodingKey {
        case ownerUserID, projectID, roomID, agentID, deliveryID, triggerMessageID
        case rootMessageID, runID, hopCount, lane
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        try self.init(
            ownerUserID: values.decode(String.self, forKey: .ownerUserID),
            projectID: values.decode(String.self, forKey: .projectID),
            roomID: values.decode(String.self, forKey: .roomID),
            agentID: values.decode(String.self, forKey: .agentID),
            deliveryID: values.decode(String.self, forKey: .deliveryID),
            triggerMessageID: values.decode(String.self, forKey: .triggerMessageID),
            rootMessageID: values.decode(String.self, forKey: .rootMessageID),
            runID: values.decode(String.self, forKey: .runID),
            hopCount: values.decode(Int.self, forKey: .hopCount),
            lane: values.decodeIfPresent(LocalAgentRunLane.self, forKey: .lane) ?? .manager
        )
    }
}

/// The single local collaboration MCP shared by every native Agent. It owns no model runtime:
/// each connection is scoped to one immutable Agent delivery identity and all communication
/// goes through the same durable room store. A future stdio/HTTP transport can delegate to this
/// server without changing its authorization or routing semantics.
public actor LocalAgentRelayMCPServer {
    private let service: NativeAgentGroupChatService
    private let limits: AgentGroupChatRoutingLimits
    private let now: @Sendable () -> Int64
    private let todoCancellationHandler: @Sendable (String) async -> Void

    public init(
        service: NativeAgentGroupChatService,
        limits: AgentGroupChatRoutingLimits = .init(),
        todoCancellationHandler: @escaping @Sendable (String) async -> Void = { _ in },
        now: @escaping @Sendable () -> Int64 = {
            Int64(Date().timeIntervalSince1970 * 1_000)
        }
    ) {
        self.service = service
        self.limits = limits
        self.todoCancellationHandler = todoCancellationHandler
        self.now = now
    }

    public func connect(
        context: LocalAgentChatRunContext,
        professions: [LocalAgentProfessionDefinition] = LocalAgentSkillCatalog.professions,
        todoPluginOptions: [LocalAgentTodoPluginOption] = []
    ) async throws -> LocalAgentChatToolProvider {
        let store = try await service.store()
        return try LocalAgentChatToolProvider(
            store: store,
            context: context,
            professions: professions,
            todoPluginOptions: todoPluginOptions,
            limits: limits,
            todoCancellationHandler: todoCancellationHandler,
            roomChangeHandler: { [service] roomID in
                await service.publishChange(.init(
                    ownerUserID: context.ownerUserID,
                    roomID: roomID,
                    agentID: context.agentID,
                    kind: .roomUpdated
                ))
            },
            now: now
        )
    }
}

private actor LocalAgentRunReferenceVault {
    struct MessageAuthority: Sendable {
        let roomID: String
        let messageID: String
    }

    struct TodoAuthority: Sendable {
        let todoID: String
        let agentID: String
        let teamRoomID: String
    }

    struct AssigneeAuthority: Sendable {
        let agentID: String
        let teamRoomID: String
    }

    struct AttachmentAuthority: Sendable {
        let roomID: String
        let messageID: String
        let attachmentID: String
    }

    struct TeamAssetAuthority: Sendable {
        let assetID: String
        let teamRoomID: String
        let revision: Int
    }

    private var conversations: [String: String] = [:]
    private var messages: [String: MessageAuthority] = [:]
    private var todos: [String: TodoAuthority] = [:]
    private var teams: [String: String] = [:]
    private var agents: [String: String] = [:]
    private var assignees: [String: AssigneeAuthority] = [:]
    private var plugins: [String: LocalAgentTodoPluginOption] = [:]
    private var attachments: [String: AttachmentAuthority] = [:]
    private var teamAssets: [String: TeamAssetAuthority] = [:]

    func conversationReference(roomID: String) -> String {
        if let existing = conversations.first(where: { $0.value == roomID })?.key {
            return existing
        }
        let reference = "conversation_\(UUID().uuidString.lowercased())"
        conversations[reference] = roomID
        return reference
    }

    func messageReference(roomID: String, messageID: String) -> String {
        if let existing = messages.first(where: {
            $0.value.roomID == roomID && $0.value.messageID == messageID
        })?.key { return existing }
        let reference = "message_\(UUID().uuidString.lowercased())"
        messages[reference] = .init(roomID: roomID, messageID: messageID)
        return reference
    }

    func messageAuthority(reference: String) -> MessageAuthority? { messages[reference] }
    func roomID(conversationReference: String) -> String? {
        conversations[conversationReference]
    }

    func todoReference(todoID: String, agentID: String, teamRoomID: String) -> String {
        if let existing = todos.first(where: { $0.value.todoID == todoID })?.key {
            return existing
        }
        let reference = "todo_\(UUID().uuidString.lowercased())"
        todos[reference] = .init(
            todoID: todoID,
            agentID: agentID,
            teamRoomID: teamRoomID
        )
        return reference
    }

    func todoAuthority(reference: String) -> TodoAuthority? { todos[reference] }

    func teamReference(teamID: String) -> String {
        if let existing = teams.first(where: { $0.value == teamID })?.key { return existing }
        let reference = "team_\(UUID().uuidString.lowercased())"
        teams[reference] = teamID
        return reference
    }

    func teamID(reference: String) -> String? { teams[reference] }

    func agentReference(agentID: String) -> String {
        if let existing = agents.first(where: { $0.value == agentID })?.key { return existing }
        let reference = "agent_\(UUID().uuidString.lowercased())"
        agents[reference] = agentID
        return reference
    }

    func agentID(reference: String) -> String? { agents[reference] }

    func assigneeReference(agentID: String, teamRoomID: String) -> String {
        if let existing = assignees.first(where: {
            $0.value.agentID == agentID && $0.value.teamRoomID == teamRoomID
        })?.key { return existing }
        let reference = "assignee_\(UUID().uuidString.lowercased())"
        assignees[reference] = .init(agentID: agentID, teamRoomID: teamRoomID)
        return reference
    }

    func assigneeAuthority(reference: String) -> AssigneeAuthority? { assignees[reference] }

    func pluginReference(option: LocalAgentTodoPluginOption) -> String {
        if let existing = plugins.first(where: { $0.value.pluginID == option.pluginID })?.key {
            return existing
        }
        let reference = "plugin_\(UUID().uuidString.lowercased())"
        plugins[reference] = option
        return reference
    }

    func plugin(reference: String) -> LocalAgentTodoPluginOption? { plugins[reference] }

    func attachmentReference(roomID: String, messageID: String, attachmentID: String) -> String {
        if let existing = attachments.first(where: {
            $0.value.roomID == roomID
                && $0.value.messageID == messageID
                && $0.value.attachmentID == attachmentID
        })?.key { return existing }
        let reference = "attachment_\(UUID().uuidString.lowercased())"
        attachments[reference] = .init(
            roomID: roomID,
            messageID: messageID,
            attachmentID: attachmentID
        )
        return reference
    }

    func attachmentAuthority(reference: String) -> AttachmentAuthority? {
        attachments[reference]
    }

    func teamAssetReference(assetID: String, teamRoomID: String, revision: Int) -> String {
        if let existing = teamAssets.first(where: {
            $0.value.assetID == assetID
                && $0.value.teamRoomID == teamRoomID
                && $0.value.revision == revision
        })?.key { return existing }
        let reference = "team_asset_\(UUID().uuidString.lowercased())"
        teamAssets[reference] = .init(
            assetID: assetID,
            teamRoomID: teamRoomID,
            revision: revision
        )
        return reference
    }

    func teamAssetAuthority(reference: String) -> TeamAssetAuthority? {
        teamAssets[reference]
    }
}

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
    public static let teamAssetUpsertToolName = "team_asset_upsert"
    public static let teamAssetArchiveToolName = "team_asset_archive"

    private let store: any AgentGroupChatStore
    private let context: LocalAgentChatRunContext
    private let professions: [LocalAgentProfessionDefinition]
    private let limits: AgentGroupChatRoutingLimits
    private let now: @Sendable () -> Int64
    private let references: LocalAgentRunReferenceVault
    private let todoPluginOptions: [LocalAgentTodoPluginOption]
    private let todoCancellationHandler: @Sendable (String) async -> Void
    private let roomChangeHandler: @Sendable (String) async -> Void

    public init(
        store: any AgentGroupChatStore,
        context: LocalAgentChatRunContext,
        professions: [LocalAgentProfessionDefinition] = LocalAgentSkillCatalog.professions,
        todoPluginOptions: [LocalAgentTodoPluginOption] = [],
        limits: AgentGroupChatRoutingLimits = .init(),
        todoCancellationHandler: @escaping @Sendable (String) async -> Void = { _ in },
        roomChangeHandler: @escaping @Sendable (String) async -> Void = { _ in },
        now: @escaping @Sendable () -> Int64 = {
            Int64(Date().timeIntervalSince1970 * 1_000)
        }
    ) throws {
        try limits.validate()
        self.store = store
        self.context = context
        self.professions = professions
        self.limits = limits
        self.now = now
        self.references = LocalAgentRunReferenceVault()
        self.todoPluginOptions = todoPluginOptions
        self.todoCancellationHandler = todoCancellationHandler
        self.roomChangeHandler = roomChangeHandler
    }

    public func definitions() async throws -> [AgentToolDefinition] {
        var definitions = Self.toolDefinitions
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
            ]
            return definitions.filter { executorTools.contains($0.name) }
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
                Self.teamAssetUpsertToolName,
                Self.teamAssetArchiveToolName,
            ]
            definitions.removeAll { projectManagerOnly.contains($0.name) }
        }
        return definitions
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
        case Self.teamAssetUpsertToolName:
            return try await upsertTeamAsset(call)
        case Self.teamAssetArchiveToolName:
            return try await archiveTeamAsset(call)
        default:
            return .failure("群聊工具不可用：\(call.name)")
        }
    }

    private func bootstrap(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        guard let room = try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID
        ), room.id == context.roomID,
           let trigger = try await store.message(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            messageID: context.triggerMessageID
           ) else { throw AgentGroupChatError.notFound }
        let members = try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID
        )
        guard let currentMember = members.first(where: { $0.agentID == context.agentID }) else {
            throw AgentGroupChatError.notMember
        }
        let agents = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        let profiles = Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
        guard let currentProfile = profiles[context.agentID] else {
            throw AgentGroupChatError.notFound
        }
        let unread = try await store.listUnreadMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            limit: 20
        )
        let conversationReference = await references.conversationReference(roomID: room.id)
        var memberResponses: [MemberResponse] = []
        for member in members {
            memberResponses.append(MemberResponse(
                agentReference: await references.agentReference(agentID: member.agentID),
                name: profiles[member.agentID]?.draft.name ?? "Agent",
                role: member.draft.role,
                responsibility: member.draft.responsibility,
                isProjectManager: room.projectManagerAgentID == member.agentID
            ))
        }
        return try Self.outcome(BootstrapResponse(
            agent: .init(
                agentReference: await references.agentReference(agentID: currentProfile.id),
                name: currentProfile.draft.name,
                role: currentMember.draft.role,
                responsibility: currentMember.draft.responsibility,
                isProjectManager: room.projectManagerAgentID == currentProfile.id
            ),
            conversationReference: conversationReference,
            roomName: room.draft.name,
            roomGoal: room.draft.goal,
            trigger: await messageResponse(trigger, profiles: profiles),
            unread: await unreadResponse(unread, profiles: profiles),
            members: memberResponses
        ))
    }

    /// Account-local discovery is deliberately separate from `relay_bootstrap`: bootstrap is
    /// scoped to the current conversation, while this snapshot answers organization questions
    /// without exposing durable database identifiers to the model.
    private func workspaceSnapshot(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        let profilesByID = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0) })
        let rooms = try await store.listRooms(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        var teamResponses: [WorkspaceTeamResponse] = []
        var teamNamesByAgentID: [String: [String]] = [:]
        for room in rooms {
            let members = try await store.listMembers(
                ownerUserID: context.ownerUserID,
                roomID: room.id
            ).filter { $0.status == .active }
            var memberResponses: [WorkspaceTeamMemberResponse] = []
            for member in members {
                let profile = profilesByID[member.agentID]
                teamNamesByAgentID[member.agentID, default: []].append(room.draft.name)
                memberResponses.append(.init(
                    agentReference: await references.agentReference(agentID: member.agentID),
                    name: profile?.draft.name ?? "Agent",
                    profession: profile?.draft.professionKey ?? LocalAgentSkillCatalog.legacyProfessionKey,
                    role: member.draft.role,
                    isProjectManager: room.projectManagerAgentID == member.agentID
                ))
            }
            teamResponses.append(.init(
                teamReference: await references.teamReference(teamID: room.id),
                name: room.draft.name,
                goal: room.draft.goal,
                hasProjectManager: room.projectManagerAgentID != nil,
                projectManager: room.projectManagerAgentID.flatMap {
                    profilesByID[$0]?.draft.name
                },
                members: memberResponses
            ))
        }
        var agentResponses: [WorkspaceAgentResponse] = []
        for profile in profiles {
            agentResponses.append(.init(
                agentReference: await references.agentReference(agentID: profile.id),
                name: profile.draft.name,
                profession: profile.draft.professionKey,
                isCurrentAgent: profile.id == context.agentID,
                teams: (teamNamesByAgentID[profile.id] ?? []).sorted()
            ))
        }
        return try Self.outcome(WorkspaceSnapshotResponse(
            agents: agentResponses,
            teams: teamResponses
        ))
    }

    private func getTrigger(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        guard let message = try await store.message(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            messageID: context.triggerMessageID
        ) else { throw AgentGroupChatError.notFound }
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        return try Self.outcome(await messageResponse(
            message,
            profiles: Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0) })
        ))
    }

    private func listMembers(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        let members = try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID
        )
        let agents = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        let profiles = Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
        let room = try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID
        )
        var response: [MemberResponse] = []
        for member in members {
            response.append(MemberResponse(
                agentReference: await references.agentReference(agentID: member.agentID),
                name: profiles[member.agentID]?.draft.name ?? "Agent",
                role: member.draft.role,
                responsibility: member.draft.responsibility,
                isProjectManager: room?.projectManagerAgentID == member.agentID
            ))
        }
        return try Self.outcome(response)
    }

    private func readUnread(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let limit = try Self.optionalInteger(arguments, key: "limit").map(Int.init) ?? 50
        let page = try await store.listUnreadMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            limit: limit
        )
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        return try Self.outcome(await unreadResponse(
            page,
            profiles: Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0) })
        ))
    }

    private func unreadResponse(
        _ page: ProjectAgentUnreadPage,
        profiles: [String: LocalAgentProfile]
    ) async -> MessagePageResponse {
        var messages: [MessageResponse] = []
        for message in page.messages {
            messages.append(await messageResponse(message, profiles: profiles))
        }
        let nextCursorReference: String?
        if let messageID = page.nextCursorMessageID {
            nextCursorReference = await references.messageReference(
                roomID: context.roomID,
                messageID: messageID
            )
        } else { nextCursorReference = nil }
        let readThroughReference: String?
        if let messageID = page.readThroughMessageID {
            readThroughReference = await references.messageReference(
                roomID: context.roomID,
                messageID: messageID
            )
        } else { readThroughReference = nil }
        return .init(
            messages: messages,
            nextCursorReference: nextCursorReference,
            hasMore: page.hasMore,
            readThroughReference: readThroughReference
        )
    }

    private func messageResponse(
        _ message: ProjectAgentMessage,
        profiles: [String: LocalAgentProfile]
    ) async -> MessageResponse {
        let senderName: String
        let senderAgentReference: String?
        switch message.senderKind {
        case .human:
            senderName = "Human"
            senderAgentReference = nil
        case .agent:
            senderName = profiles[message.senderID]?.draft.name ?? "Agent"
            senderAgentReference = await references.agentReference(agentID: message.senderID)
        case .system:
            senderName = "System"
            senderAgentReference = nil
        }
        var attachments: [AttachmentResponse] = []
        for attachment in message.attachmentItems {
            attachments.append(.init(
                attachmentReference: await references.attachmentReference(
                    roomID: message.roomID,
                    messageID: message.id,
                    attachmentID: attachment.id
                ),
                name: attachment.name,
                mimeType: attachment.mimeType,
                size: attachment.size,
                kind: attachment.kind.rawValue
            ))
        }
        let replyToReference: String?
        if let replyToMessageID = message.replyToMessageID {
            replyToReference = await references.messageReference(
                roomID: message.roomID,
                messageID: replyToMessageID
            )
        } else { replyToReference = nil }
        return .init(
            messageReference: await references.messageReference(
                roomID: message.roomID,
                messageID: message.id
            ),
            sender: senderName,
            senderAgentReference: senderAgentReference,
            content: message.content,
            replyToMessageReference: replyToReference,
            attachments: attachments,
            createdAtUnixMs: message.createdAtUnixMs
        )
    }

    private func readAllUnread(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let limit = try Self.optionalInteger(arguments, key: "limit").map(Int.init) ?? 200
        let conversations = try await store.readAllUnreadMessagesAndMarkRead(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            limit: limit,
            nowUnixMs: now()
        )
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let names = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0.draft.name) })
        var response: [InboxConversationResponse] = []
        for conversation in conversations {
            let conversationReference = await references.conversationReference(
                roomID: conversation.room.id
            )
            var messages: [InboxMessageResponse] = []
            for message in conversation.messages {
                let messageReference = await references.messageReference(
                    roomID: conversation.room.id,
                    messageID: message.id
                )
                let sender = switch message.senderKind {
                case .human: "Human"
                case .agent: names[message.senderID] ?? "Agent"
                case .system: "System"
                }
                var attachments: [AttachmentResponse] = []
                for attachment in message.attachmentItems {
                    attachments.append(.init(
                        attachmentReference: await references.attachmentReference(
                            roomID: conversation.room.id,
                            messageID: message.id,
                            attachmentID: attachment.id
                        ),
                        name: attachment.name,
                        mimeType: attachment.mimeType,
                        size: attachment.size,
                        kind: attachment.kind.rawValue
                    ))
                }
                messages.append(.init(
                    messageReference: messageReference,
                    sender: sender,
                    content: message.content,
                    attachments: attachments,
                    createdAtUnixMs: message.createdAtUnixMs
                ))
            }
            response.append(.init(
                conversationReference: conversationReference,
                name: conversation.room.draft.name,
                kind: conversation.room.conversationKind.rawValue,
                messages: messages
            ))
        }
        return try Self.outcome(InboxResponse(
            conversations: response,
            messageCount: response.reduce(0) { $0 + $1.messages.count },
            markedRead: true
        ))
    }

    private func sendInboxMessage(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let conversationReference = try Self.requiredString(
            arguments,
            key: "conversation_ref"
        )
        let replyReference = try Self.requiredString(arguments, key: "reply_to_message_ref")
        guard let roomID = await references.roomID(
            conversationReference: conversationReference
        ), let source = await references.messageAuthority(reference: replyReference),
           source.roomID == roomID,
           let replyMessage = try await store.message(
               ownerUserID: context.ownerUserID,
               roomID: roomID,
               messageID: source.messageID
           ) else { throw AgentGroupChatError.invalidField("inbox_reference") }
        let notifyProjectManager = try Self.optionalBoolean(
            arguments,
            key: "notify_project_manager"
        ) ?? false
        var mentionedAgentIDs: [String] = []
        if notifyProjectManager {
            guard let room = try await store.room(
                ownerUserID: context.ownerUserID,
                roomID: roomID
            ), room.conversationKind == .projectTeam,
            let projectManagerAgentID = room.projectManagerAgentID else {
                return Self.structuredFailure(
                    code: "project_manager_unavailable",
                    field: "notify_project_manager",
                    message: "该会话不是项目团队，或团队尚未明确指定项目经理。",
                    retryable: false
                )
            }
            mentionedAgentIDs = [projectManagerAgentID]
        }
        let post = try await store.postMessage(
            ownerUserID: context.ownerUserID,
            roomID: roomID,
            draft: .init(
                senderKind: .agent,
                senderID: context.agentID,
                content: try Self.requiredString(arguments, key: "content"),
                mentionedAgentIDs: mentionedAgentIDs,
                replyToMessageID: replyMessage.id,
                sourceRunID: context.runID,
                causationID: context.deliveryID,
                rootMessageID: replyMessage.rootMessageID,
                hopCount: min(64, replyMessage.hopCount + 1)
            ),
            limits: limits
        )
        await roomChangeHandler(roomID)
        return try Self.outcome(InboxSendResponse(
            sent: true,
            notifiedProjectManager: notifyProjectManager,
            conversationReference: conversationReference,
            replyToMessageReference: replyReference,
            spawnedDeliveryCount: post.deliveries.count,
            routingStopReason: post.routingStopReason
        ))
    }

    private func listTodos(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let includeTerminal = try Self.optionalBoolean(
            arguments,
            key: "include_terminal"
        ) ?? false
        return try Self.outcome(try await todoResponses(includeTerminal: includeTerminal))
    }

    private func todoScheduleState(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        let state = try await store.agentTodoScheduleState(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID
        )
        let runningResponse: TodoResponse?
        if let runningTodo = state.runningTodo {
            runningResponse = try await todoResponse(runningTodo)
        } else {
            runningResponse = nil
        }
        let readyResponse: TodoResponse?
        if let readyTodo = state.readyTodo {
            readyResponse = try await todoResponse(readyTodo)
        } else {
            readyResponse = nil
        }
        return try Self.outcome(TodoScheduleStateResponse(
            state: state.runningTodo != nil ? "busy" : (state.readyTodo != nil ? "ready" : "idle"),
            runningTodo: runningResponse,
            readyTodo: readyResponse
        ))
    }

    private func startNextTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        let delivery = try await store.startNextReadyAgentTodo(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            nowUnixMs: now()
        )
        if let delivery,
           let todo = try await store.todoForDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: delivery.id
           ) {
            return try Self.outcome(TodoStartNextResponse(
                status: "started",
                todo: try await todoResponse(todo)
            ))
        }
        let state = try await store.agentTodoScheduleState(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID
        )
        let runningResponse: TodoResponse?
        if let runningTodo = state.runningTodo {
            runningResponse = try await todoResponse(runningTodo)
        } else {
            runningResponse = nil
        }
        return try Self.outcome(TodoStartNextResponse(
            status: state.runningTodo != nil ? "executor_busy" : "no_ready_todo",
            todo: runningResponse
        ))
    }

    private func listTeamAssets(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let teamRoomID: String
        if context.lane == .executor {
            guard let todo = try await currentExecutionTodo() else {
                return Self.structuredFailure(
                    code: "todo_execution_context_mismatch",
                    field: "delivery",
                    message: "当前执行线程没有有效的团队资产边界。",
                    retryable: false
                )
            }
            teamRoomID = todo.teamRoomID
        } else if let teamReference = try Self.optionalString(arguments, key: "team_ref") {
            guard let resolved = await references.teamID(reference: teamReference) else {
                return Self.structuredFailure(
                    code: "invalid_team_ref",
                    field: "team_ref",
                    message: "团队引用无效或已经过期，请重新调用 agent_workspace_snapshot。",
                    retryable: true,
                    nextTool: Self.workspaceSnapshotToolName
                )
            }
            teamRoomID = resolved
        } else if try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID
        )?.conversationKind == .projectTeam {
            teamRoomID = context.roomID
        } else {
            return Self.structuredFailure(
                code: "team_ref_required",
                field: "team_ref",
                message: "当前是私聊，请先调用 agent_workspace_snapshot 并选择一个项目团队。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        guard try await isTeamMember(teamRoomID: teamRoomID) else {
            return Self.structuredFailure(
                code: "team_membership_required",
                field: "team_ref",
                message: "当前 Agent 不是该团队成员，不能读取团队共享资产。",
                retryable: false
            )
        }
        var response: [TeamAssetSummaryResponse] = []
        if context.lane == .executor, let todo = try await currentExecutionTodo() {
            for snapshot in try await store.listTodoTeamAssetSnapshots(
                ownerUserID: context.ownerUserID,
                todoID: todo.id
            ) {
                response.append(.init(
                    assetReference: await references.teamAssetReference(
                        assetID: snapshot.assetID,
                        teamRoomID: snapshot.teamRoomID,
                        revision: snapshot.revision
                    ),
                    category: snapshot.category.rawValue,
                    title: snapshot.title,
                    revision: snapshot.revision,
                    updatedAtUnixMs: snapshot.capturedAtUnixMs
                ))
            }
        } else {
            for asset in try await store.listTeamAssets(
                ownerUserID: context.ownerUserID,
                teamRoomID: teamRoomID,
                includeArchived: false
            ) {
                response.append(.init(
                    assetReference: await references.teamAssetReference(
                        assetID: asset.id,
                        teamRoomID: asset.teamRoomID,
                        revision: asset.revision
                    ),
                    category: asset.category.rawValue,
                    title: asset.title,
                    revision: asset.revision,
                    updatedAtUnixMs: asset.updatedAtUnixMs
                ))
            }
        }
        return try Self.outcome(response)
    }

    private func getTeamAsset(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let reference = try Self.requiredString(arguments, key: "asset_ref")
        guard let authority = await references.teamAssetAuthority(reference: reference),
              try await isTeamMember(teamRoomID: authority.teamRoomID) else {
            return Self.structuredFailure(
                code: "invalid_team_asset_ref",
                field: "asset_ref",
                message: "团队资产引用无效、已归档或已经过期，请重新调用 team_asset_list。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        if context.lane == .executor, let todo = try await currentExecutionTodo() {
            guard let snapshot = try await store.todoTeamAssetSnapshot(
                ownerUserID: context.ownerUserID,
                todoID: todo.id,
                assetID: authority.assetID,
                revision: authority.revision
            ) else {
                return Self.structuredFailure(
                    code: "invalid_team_asset_ref",
                    field: "asset_ref",
                    message: "该资产不属于当前 Todo 启动时固化的团队上下文。",
                    retryable: true,
                    nextTool: Self.teamAssetListToolName
                )
            }
            return try Self.outcome(TeamAssetDetailResponse(
                assetReference: reference,
                category: snapshot.category.rawValue,
                title: snapshot.title,
                markdown: snapshot.markdown,
                revision: snapshot.revision,
                updatedAtUnixMs: snapshot.capturedAtUnixMs
            ))
        }
        guard let asset = try await store.teamAsset(
            ownerUserID: context.ownerUserID,
            teamRoomID: authority.teamRoomID,
            assetID: authority.assetID
        ), asset.status == .active else {
            return Self.structuredFailure(
                code: "invalid_team_asset_ref",
                field: "asset_ref",
                message: "团队资产已经归档，请重新调用 team_asset_list。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        guard asset.revision == authority.revision else {
            return Self.structuredFailure(
                code: "team_asset_revision_changed",
                field: "asset_ref",
                message: "团队资产已经产生新修订，请重新调用 team_asset_list 后读取最新版本。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        return try Self.outcome(TeamAssetDetailResponse(
            assetReference: reference,
            category: asset.category.rawValue,
            title: asset.title,
            markdown: asset.markdown,
            revision: asset.revision,
            updatedAtUnixMs: asset.updatedAtUnixMs
        ))
    }

    private func upsertTeamAsset(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard context.lane == .manager else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let assetReference = try Self.optionalString(arguments, key: "asset_ref")
        let authority: LocalAgentRunReferenceVault.TeamAssetAuthority? = if let assetReference {
            await references.teamAssetAuthority(reference: assetReference)
        } else {
            nil
        }
        if assetReference != nil, authority == nil {
            return Self.structuredFailure(
                code: "invalid_team_asset_ref",
                field: "asset_ref",
                message: "团队资产引用无效或已经过期，请重新调用 team_asset_list。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        let teamRoomID: String
        if let authority {
            teamRoomID = authority.teamRoomID
        } else {
            let teamReference = try Self.requiredString(arguments, key: "team_ref")
            guard let resolved = await references.teamID(reference: teamReference) else {
                return Self.structuredFailure(
                    code: "invalid_team_ref",
                    field: "team_ref",
                    message: "团队引用无效或已经过期，请重新调用 agent_workspace_snapshot。",
                    retryable: true,
                    nextTool: Self.workspaceSnapshotToolName
                )
            }
            teamRoomID = resolved
        }
        guard try await isProjectManager(teamRoomID: teamRoomID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "team_ref",
                message: "只有该团队明确指定的项目经理可以维护共享资产。",
                retryable: false
            )
        }
        guard let category = LocalAgentTeamAssetCategory(
            rawValue: try Self.requiredString(arguments, key: "category")
        ) else {
            return Self.structuredFailure(
                code: "invalid_team_asset_category",
                field: "category",
                message: "共享资产分类无效。",
                retryable: true
            )
        }
        let expectedRevision = try Self.optionalInteger(arguments, key: "expected_revision").map(Int.init)
        if let authority, expectedRevision != authority.revision {
            return Self.structuredFailure(
                code: "team_asset_revision_required",
                field: "expected_revision",
                message: "更新共享资产必须使用 team_asset_list 返回的当前 revision。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        let asset = try await store.upsertTeamAsset(
            ownerUserID: context.ownerUserID,
            teamRoomID: teamRoomID,
            assetID: authority?.assetID,
            editorAgentID: context.agentID,
            category: category,
            title: try Self.requiredString(arguments, key: "title"),
            markdown: try Self.requiredString(arguments, key: "markdown"),
            expectedRevision: expectedRevision,
            nowUnixMs: now()
        )
        let reference = await references.teamAssetReference(
            assetID: asset.id,
            teamRoomID: asset.teamRoomID,
            revision: asset.revision
        )
        return try Self.outcome(TeamAssetDetailResponse(
            assetReference: reference,
            category: asset.category.rawValue,
            title: asset.title,
            markdown: asset.markdown,
            revision: asset.revision,
            updatedAtUnixMs: asset.updatedAtUnixMs
        ))
    }

    private func archiveTeamAsset(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard context.lane == .manager else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let reference = try Self.requiredString(arguments, key: "asset_ref")
        guard let authority = await references.teamAssetAuthority(reference: reference) else {
            return Self.structuredFailure(
                code: "invalid_team_asset_ref",
                field: "asset_ref",
                message: "团队资产引用无效或已经过期，请重新调用 team_asset_list。",
                retryable: true,
                nextTool: Self.teamAssetListToolName
            )
        }
        guard try await isProjectManager(teamRoomID: authority.teamRoomID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "asset_ref",
                message: "只有该团队明确指定的项目经理可以归档共享资产。",
                retryable: false
            )
        }
        let asset = try await store.archiveTeamAsset(
            ownerUserID: context.ownerUserID,
            teamRoomID: authority.teamRoomID,
            assetID: authority.assetID,
            editorAgentID: context.agentID,
            expectedRevision: authority.revision,
            nowUnixMs: now()
        )
        return try Self.outcome(["status": asset.status.rawValue])
    }

    private func todoExecutionOptions(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        let rooms = try await store.listRooms(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        let profilesByID = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0) })
        var teams: [TodoTeamOptionResponse] = []
        for room in rooms where room.conversationKind == .projectTeam {
            guard room.projectManagerAgentID == context.agentID else { continue }
            let members = try await store.listMembers(
                ownerUserID: context.ownerUserID,
                roomID: room.id
            )
            var assignees: [TodoAssigneeOptionResponse] = []
            for member in members where member.status == .active {
                guard let profile = profilesByID[member.agentID] else { continue }
                assignees.append(.init(
                    assigneeReference: await references.assigneeReference(
                        agentID: member.agentID,
                        teamRoomID: room.id
                    ),
                    name: profile.draft.name,
                    profession: profile.draft.professionKey,
                    role: member.draft.role,
                    isProjectManager: room.projectManagerAgentID == member.agentID
                ))
            }
            teams.append(.init(
                teamReference: await references.teamReference(teamID: room.id),
                name: room.draft.name,
                goal: room.draft.goal,
                assignees: assignees
            ))
        }
        var plugins: [TodoPluginOptionResponse] = []
        for option in todoPluginOptions {
            plugins.append(.init(
                pluginReference: await references.pluginReference(option: option),
                name: option.displayName,
                description: option.description
            ))
        }
        return try Self.outcome(TodoExecutionOptionsResponse(
            teams: teams,
            builtinCapabilities: LocalAgentTodoBuiltinCapability.allCases.map(\.rawValue),
            plugins: plugins
        ))
    }

    private func todoDependencyOptions(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let teamReference = try Self.requiredString(arguments, key: "team_ref")
        guard let teamID = await references.teamID(reference: teamReference),
              try await isProjectManager(teamRoomID: teamID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "team_ref",
                message: "只有该团队明确指定的项目经理可以管理任务与前置依赖。",
                retryable: true,
                nextTool: Self.todoExecutionOptionsToolName
            )
        }
        let todos = try await store.listTeamTodos(
            ownerUserID: context.ownerUserID,
            teamRoomID: teamID,
            includeTerminal: true
        ).filter { $0.status != .cancelled }
        var response: [TodoDependencyOptionResponse] = []
        for todo in todos {
            let profile = try await store.listAgents(
                ownerUserID: context.ownerUserID,
                includeArchived: true
            ).first(where: { $0.id == todo.agentID })
            response.append(.init(
                todoReference: await references.todoReference(
                    todoID: todo.id,
                    agentID: todo.agentID,
                    teamRoomID: todo.teamRoomID
                ),
                title: todo.title,
                assignee: profile?.draft.name ?? "Agent",
                status: todo.status.rawValue,
                blockedReason: todo.blockedReason,
                result: todo.result
            ))
        }
        return try Self.outcome(response)
    }

    private func addTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let teamReference = try Self.requiredString(arguments, key: "team_ref")
        guard let teamID = await references.teamID(reference: teamReference) else {
            return Self.structuredFailure(
                code: "invalid_team_ref",
                field: "team_ref",
                message: "团队选项无效或已经过期。请重新调用 todo_execution_options。",
                retryable: true,
                nextTool: Self.todoExecutionOptionsToolName
            )
        }
        guard try await isProjectManager(teamRoomID: teamID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "team_ref",
                message: "只有该团队明确指定的项目经理可以创建和分配团队任务。",
                retryable: false
            )
        }
        let assigneeID: String
        if let assigneeReference = try Self.optionalString(arguments, key: "assignee_ref") {
            guard let authority = await references.assigneeAuthority(
                reference: assigneeReference
            ), authority.teamRoomID == teamID else {
                return Self.structuredFailure(
                    code: "invalid_assignee_ref",
                    field: "assignee_ref",
                    message: "负责人选项无效、已过期或不属于所选团队。请重新读取执行选项。",
                    retryable: true,
                    nextTool: Self.todoExecutionOptionsToolName
                )
            }
            assigneeID = authority.agentID
        } else {
            assigneeID = context.agentID
        }
        let dependencyReferences = try Self.optionalStringArray(
            arguments,
            key: "depends_on_todo_refs"
        )
        var dependencies: [LocalAgentTodoDependencyDraft] = []
        var dependencyIDs = Set<String>()
        for (index, reference) in dependencyReferences.enumerated() {
            guard let authority = await references.todoAuthority(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_dependency_todo_ref",
                    field: "depends_on_todo_refs[\(index)]",
                    message: "前置任务引用无效或已经过期。请重新读取团队前置任务选项。",
                    retryable: true,
                    nextTool: Self.todoDependencyOptionsToolName
                )
            }
            guard authority.teamRoomID == teamID else {
                return Self.structuredFailure(
                    code: "cross_team_dependency",
                    field: "depends_on_todo_refs[\(index)]",
                    message: "前置任务必须与当前任务属于同一个项目团队。",
                    retryable: true,
                    nextTool: Self.todoDependencyOptionsToolName
                )
            }
            guard dependencyIDs.insert(authority.todoID).inserted else {
                return Self.structuredFailure(
                    code: "duplicate_dependency",
                    field: "depends_on_todo_refs",
                    message: "同一个前置任务不能重复添加。",
                    retryable: true
                )
            }
            dependencies.append(.init(
                prerequisiteTodoID: authority.todoID,
                prerequisiteAgentID: authority.agentID
            ))
        }
        let sourceReferences = try Self.optionalStringArray(
            arguments,
            key: "source_message_refs"
        )
        guard !sourceReferences.isEmpty else {
            return Self.structuredFailure(
                code: "missing_source_messages",
                field: "source_message_refs",
                message: "Todo 必须关联至少一条本轮已经读取的来源消息。",
                retryable: true,
                nextTool: Self.readAllUnreadToolName
            )
        }
        var sources: [LocalAgentTodoSourceDraft] = []
        for reference in sourceReferences {
            guard let source = await references.messageAuthority(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_source_message_ref",
                    field: "source_message_refs",
                    message: "来源消息引用无效、已过期，或不属于当前 Agent 的本轮收件箱。",
                    retryable: true,
                    nextTool: Self.readAllUnreadToolName
                )
            }
            sources.append(.init(roomID: source.roomID, messageID: source.messageID))
        }
        let requestedKinds = try Self.optionalStringArray(
            arguments,
            key: "builtin_capabilities"
        )
        var builtinCapabilities: [LocalAgentTodoBuiltinCapability] = []
        for rawValue in requestedKinds {
            guard let capability = LocalAgentTodoBuiltinCapability(rawValue: rawValue) else {
                return Self.structuredFailure(
                    code: "unsupported_builtin_capability",
                    field: "builtin_capabilities",
                    message: "请求了客户端未提供的基础能力。请重新读取执行选项。",
                    retryable: true,
                    nextTool: Self.todoExecutionOptionsToolName
                )
            }
            builtinCapabilities.append(capability)
        }
        guard Set(builtinCapabilities).count == builtinCapabilities.count else {
            return Self.structuredFailure(
                code: "duplicate_builtin_capability",
                field: "builtin_capabilities",
                message: "同一种基础能力不能重复选择。",
                retryable: true
            )
        }
        let requiresExecution = try Self.optionalBoolean(
            arguments,
            key: "requires_execution"
        ) ?? true
        if !requiresExecution,
           builtinCapabilities.contains(where: { $0 != .projectRead }) {
            return Self.structuredFailure(
                code: "execution_required",
                field: "requires_execution",
                message: "文件写入或终端能力需要执行环境，请将 requires_execution 设为 true。",
                retryable: true
            )
        }
        let pluginHints = try Self.optionalObjectArray(arguments, key: "plugin_hints")
        var pluginSelections: [LocalAgentTodoPluginSelection] = []
        var selectedPluginIDs = Set<String>()
        for (index, hint) in pluginHints.enumerated() {
            guard let reference = hint["plugin_ref"] as? String,
                  let option = await references.plugin(reference: reference) else {
                return Self.structuredFailure(
                    code: "plugin_not_selectable",
                    field: "plugin_hints[\(index)].plugin_ref",
                    message: "Plugin 选项无效、已停用或已经过期。请重新读取本机执行选项。",
                    retryable: true,
                    nextTool: Self.todoExecutionOptionsToolName
                )
            }
            guard selectedPluginIDs.insert(option.pluginID).inserted else {
                return Self.structuredFailure(
                    code: "duplicate_plugin",
                    field: "plugin_hints[\(index)].plugin_ref",
                    message: "同一个 Plugin 不能被重复选择。",
                    retryable: true
                )
            }
            let reason = (hint["reason"] as? String) ?? ""
            pluginSelections.append(.init(
                pluginID: option.pluginID,
                displayName: option.displayName,
                reason: reason
            ))
        }
        let expectedOutputs = try Self.optionalStringArray(arguments, key: "expected_outputs")
        let acceptanceCriteria = try Self.optionalStringArray(
            arguments,
            key: "acceptance_criteria"
        )
        guard !expectedOutputs.isEmpty, !acceptanceCriteria.isEmpty else {
            return Self.structuredFailure(
                code: "incomplete_execution_contract",
                field: expectedOutputs.isEmpty ? "expected_outputs" : "acceptance_criteria",
                message: "创建 Todo 必须明确交付物和可核验的验收条件。",
                retryable: true
            )
        }
        let executionContract = LocalAgentTodoExecutionContract(
            objective: try Self.requiredString(arguments, key: "objective"),
            scope: try Self.requiredString(arguments, key: "scope"),
            expectedOutputs: expectedOutputs,
            acceptanceCriteria: acceptanceCriteria,
            constraints: try Self.optionalStringArray(arguments, key: "constraints")
        )
        let createdAt = now()
        do {
            let primary = sources[0]
            let todo = try await store.createAgentTodo(
                ownerUserID: context.ownerUserID,
                agentID: assigneeID,
                requestKey: call.id,
                draft: .init(
                    title: try Self.requiredString(arguments, key: "title"),
                    detail: try Self.optionalString(arguments, key: "detail") ?? "",
                    priority: Int(try Self.optionalInteger(arguments, key: "priority") ?? 50),
                    teamRoomID: teamID,
                    sourceRoomID: primary.roomID,
                    sourceMessageID: primary.messageID,
                    additionalSources: Array(sources.dropFirst()),
                    dependencies: dependencies,
                    executionPlan: .init(
                        requiresExecution: requiresExecution,
                        builtinCapabilities: builtinCapabilities,
                        plugins: pluginSelections,
                        selectionRevision: "local-capability-catalog-v1",
                        selectedAtUnixMs: createdAt
                    ),
                    executionContract: executionContract,
                    creatorAgentID: context.agentID
                ),
                nowUnixMs: createdAt
            )
            if assigneeID != context.agentID {
                _ = try await store.enqueueAgentTodoReady(
                    ownerUserID: context.ownerUserID,
                    agentID: assigneeID,
                    todoID: todo.id,
                    nowUnixMs: createdAt
                )
            }
            return try Self.outcome(try await todoResponse(todo))
        } catch let error as AgentGroupChatError {
            return Self.structuredFailure(
                code: Self.errorCode(error),
                field: Self.errorField(error),
                message: error.localizedDescription,
                retryable: error != .permissionDenied
            )
        }
    }

    private func updateTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let todoReference = try Self.requiredString(arguments, key: "todo_ref")
        guard let authority = await references.todoAuthority(reference: todoReference) else {
            return Self.structuredFailure(
                code: "invalid_todo_ref",
                field: "todo_ref",
                message: "Todo 引用无效或已经过期，请重新调用 todo_list。",
                retryable: true,
                nextTool: Self.todoListToolName
            )
        }
        guard try await isProjectManager(teamRoomID: authority.teamRoomID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "todo_ref",
                message: "只有该团队明确指定的项目经理可以修改团队任务。",
                retryable: false
            )
        }
        guard let existingTodo = try await store.agentTodo(
            ownerUserID: context.ownerUserID,
            agentID: authority.agentID,
            todoID: authority.todoID
        ) else { throw AgentGroupChatError.notFound }
        let status = try Self.optionalString(arguments, key: "status").flatMap(
            LocalAgentTodoStatus.init(rawValue:)
        )
        if arguments["status"] != nil, status == nil {
            return Self.structuredFailure(
                code: "invalid_todo_status",
                field: "status",
                message: "通讯线程只能把 Todo 重新置为 pending，或将它取消。",
                retryable: true
            )
        }
        if let status, status != .pending && status != .cancelled {
            return Self.structuredFailure(
                code: "executor_owned_status",
                field: "status",
                message: "completed 和 blocked 状态由独立 Todo 执行线程写入，通讯线程不能代替执行。",
                retryable: true
            )
        }
        let sourceReferences = try Self.optionalStringArray(
            arguments,
            key: "source_message_refs"
        )
        var linkedSources: [LocalAgentTodoSourceDraft] = []
        let relation: LocalAgentTodoSourceRelation = arguments["priority"] == nil
            ? .updated
            : .reprioritized
        for reference in sourceReferences {
            guard let source = await references.messageAuthority(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_source_message_ref",
                    field: "source_message_refs",
                    message: "补充来源消息无效、已过期，或不属于当前 Agent 的本轮收件箱。",
                    retryable: true,
                    nextTool: Self.readAllUnreadToolName
                )
            }
            linkedSources.append(.init(
                roomID: source.roomID,
                messageID: source.messageID,
                relation: relation
            ))
        }
        var dependencyDrafts: [LocalAgentTodoDependencyDraft]?
        if arguments["depends_on_todo_refs"] != nil {
            let dependencyReferences = try Self.optionalStringArray(
                arguments,
                key: "depends_on_todo_refs"
            )
            var parsed: [LocalAgentTodoDependencyDraft] = []
            var dependencyIDs = Set<String>()
            for (index, reference) in dependencyReferences.enumerated() {
                guard let prerequisite = await references.todoAuthority(reference: reference) else {
                    return Self.structuredFailure(
                        code: "invalid_dependency_todo_ref",
                        field: "depends_on_todo_refs[\(index)]",
                        message: "前置任务引用无效或已经过期。请重新读取团队前置任务选项。",
                        retryable: true,
                        nextTool: Self.todoDependencyOptionsToolName
                    )
                }
                guard prerequisite.teamRoomID == authority.teamRoomID else {
                    return Self.structuredFailure(
                        code: "cross_team_dependency",
                        field: "depends_on_todo_refs[\(index)]",
                        message: "前置任务必须与当前任务属于同一个项目团队。",
                        retryable: true,
                        nextTool: Self.todoDependencyOptionsToolName
                    )
                }
                guard prerequisite.todoID != authority.todoID else {
                    return Self.structuredFailure(
                        code: "self_dependency",
                        field: "depends_on_todo_refs[\(index)]",
                        message: "任务不能依赖自己。",
                        retryable: true
                    )
                }
                guard dependencyIDs.insert(prerequisite.todoID).inserted else {
                    return Self.structuredFailure(
                        code: "duplicate_dependency",
                        field: "depends_on_todo_refs",
                        message: "同一个前置任务不能重复添加。",
                        retryable: true
                    )
                }
                parsed.append(.init(
                    prerequisiteTodoID: prerequisite.todoID,
                    prerequisiteAgentID: prerequisite.agentID
                ))
            }
            dependencyDrafts = parsed
        }
        let timestamp = now()
        let contractKeys = [
            "objective", "scope", "expected_outputs", "acceptance_criteria", "constraints",
        ]
        let executionContract: LocalAgentTodoExecutionContract?
        if contractKeys.contains(where: { arguments[$0] != nil }) {
            executionContract = .init(
                objective: try Self.optionalString(arguments, key: "objective")
                    ?? existingTodo.executionContract.objective,
                scope: try Self.optionalString(arguments, key: "scope")
                    ?? existingTodo.executionContract.scope,
                expectedOutputs: arguments["expected_outputs"] == nil
                    ? existingTodo.executionContract.expectedOutputs
                    : try Self.optionalStringArray(arguments, key: "expected_outputs"),
                acceptanceCriteria: arguments["acceptance_criteria"] == nil
                    ? existingTodo.executionContract.acceptanceCriteria
                    : try Self.optionalStringArray(arguments, key: "acceptance_criteria"),
                constraints: arguments["constraints"] == nil
                    ? existingTodo.executionContract.constraints
                    : try Self.optionalStringArray(arguments, key: "constraints")
            )
        } else {
            executionContract = nil
        }
        let todo = try await store.updateAgentTodo(
            ownerUserID: context.ownerUserID,
            agentID: authority.agentID,
            todoID: authority.todoID,
            update: .init(
                title: try Self.optionalString(arguments, key: "title"),
                detail: try Self.optionalString(arguments, key: "detail"),
                priority: try Self.optionalInteger(arguments, key: "priority").map(Int.init),
                status: status,
                blockedReason: status == .pending ? "" : nil,
                result: nil,
                executionContract: executionContract
            ),
            nowUnixMs: timestamp
        )
        if status == .cancelled, existingTodo.status != .cancelled {
            await todoCancellationHandler(todo.id)
            _ = try await store.appendAgentTodoProgress(
                ownerUserID: context.ownerUserID,
                agentID: authority.agentID,
                todoID: authority.todoID,
                kind: .cancelled,
                runID: context.runID,
                stage: "cancelled",
                detail: "项目经理已停止该任务。",
                nowUnixMs: timestamp
            )
            _ = try await store.enqueueAgentTodoStatus(
                ownerUserID: context.ownerUserID,
                agentID: authority.agentID,
                todoID: authority.todoID,
                excludingAgentID: context.agentID,
                nowUnixMs: timestamp
            )
        }
        if !linkedSources.isEmpty {
            _ = try await store.linkAgentTodoSources(
                ownerUserID: context.ownerUserID,
                agentID: authority.agentID,
                todoID: authority.todoID,
                sources: linkedSources,
                nowUnixMs: timestamp
            )
        }
        if let dependencyDrafts {
            do {
                _ = try await store.setAgentTodoDependencies(
                    ownerUserID: context.ownerUserID,
                    agentID: authority.agentID,
                    todoID: authority.todoID,
                    dependencies: dependencyDrafts,
                    nowUnixMs: timestamp
                )
            } catch let error as AgentGroupChatError {
                return Self.structuredFailure(
                    code: error == .invalidField("todoDependencyCycle")
                        ? "dependency_cycle" : Self.errorCode(error),
                    field: Self.errorField(error) ?? "depends_on_todo_refs",
                    message: error == .invalidField("todoDependencyCycle")
                        ? "这些前置关系会形成依赖环，请重新拆分或调整依赖。"
                        : error.localizedDescription,
                    retryable: true,
                    nextTool: Self.todoDependencyOptionsToolName
                )
            }
        }
        if todo.status == .pending, authority.agentID != context.agentID {
            _ = try await store.enqueueAgentTodoReady(
                ownerUserID: context.ownerUserID,
                agentID: authority.agentID,
                todoID: authority.todoID,
                nowUnixMs: timestamp
            )
        }
        return try Self.outcome(try await todoResponse(todo))
    }

    private func reorderTodos(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let todoReferences = try Self.optionalStringArray(arguments, key: "todo_refs")
        guard !todoReferences.isEmpty else {
            throw AgentGroupChatError.invalidField("todo_refs")
        }
        var todoIDs: [String] = []
        var teamRoomID: String?
        for reference in todoReferences {
            guard let authority = await references.todoAuthority(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_todo_ref",
                    field: "todo_refs",
                    message: "Todo 引用无效或已经过期，请重新调用 todo_list。",
                    retryable: true,
                    nextTool: Self.todoListToolName
                )
            }
            if let teamRoomID, teamRoomID != authority.teamRoomID {
                return Self.structuredFailure(
                    code: "cross_team_reorder",
                    field: "todo_refs",
                    message: "一次只能调整同一个团队任务板的顺序。",
                    retryable: true
                )
            }
            teamRoomID = authority.teamRoomID
            todoIDs.append(authority.todoID)
        }
        guard let teamRoomID, try await isProjectManager(teamRoomID: teamRoomID) else {
            return Self.structuredFailure(
                code: "project_manager_required",
                field: "todo_refs",
                message: "只有该团队明确指定的项目经理可以调整团队任务顺序。",
                retryable: false
            )
        }
        let todos = try await store.reorderTeamTodos(
            ownerUserID: context.ownerUserID,
            teamRoomID: teamRoomID,
            todoIDs: todoIDs,
            nowUnixMs: now()
        )
        var response: [TodoResponse] = []
        for todo in todos { response.append(try await todoResponse(todo)) }
        return try Self.outcome(response)
    }

    private func todoGetContext(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        guard let todo = try await store.todoForDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), todo.agentID == context.agentID, todo.teamRoomID == context.roomID else {
            return Self.structuredFailure(
                code: "todo_execution_context_mismatch",
                field: "delivery",
                message: "当前执行线程没有有效的 Todo、Agent 或团队绑定。",
                retryable: false
            )
        }
        let sources = try await store.listAgentTodoSources(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id
        )
        var sourceMessages: [TodoSourceMessageResponse] = []
        for source in sources {
            guard let message = try await store.message(
                ownerUserID: context.ownerUserID,
                roomID: source.conversationID,
                messageID: source.messageID
            ) else { continue }
            sourceMessages.append(.init(
                relation: source.relation.rawValue,
                content: message.content,
                attachmentNames: message.attachmentItems.map(\.name),
                createdAtUnixMs: message.createdAtUnixMs
            ))
        }
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let names = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0.draft.name) })
        var prerequisites: [TodoDependencyResponse] = []
        for dependency in try await store.listAgentTodoDependencies(
            ownerUserID: context.ownerUserID,
            agentID: todo.agentID,
            todoID: todo.id
        ) {
            guard let prerequisite = try await store.agentTodo(
                ownerUserID: context.ownerUserID,
                agentID: dependency.prerequisiteAgentID,
                todoID: dependency.prerequisiteTodoID
            ) else { continue }
            prerequisites.append(.init(
                todoReference: await references.todoReference(
                    todoID: prerequisite.id,
                    agentID: prerequisite.agentID,
                    teamRoomID: prerequisite.teamRoomID
                ),
                title: prerequisite.title,
                assignee: names[prerequisite.agentID] ?? "Agent",
                status: prerequisite.status.rawValue,
                blockedReason: prerequisite.blockedReason,
                result: prerequisite.result
            ))
        }
        var teamAssets: [TeamAssetSummaryResponse] = []
        for asset in try await store.listTodoTeamAssetSnapshots(
            ownerUserID: context.ownerUserID,
            todoID: todo.id
        ) {
            teamAssets.append(.init(
                assetReference: await references.teamAssetReference(
                    assetID: asset.assetID,
                    teamRoomID: asset.teamRoomID,
                    revision: asset.revision
                ),
                category: asset.category.rawValue,
                title: asset.title,
                revision: asset.revision,
                updatedAtUnixMs: asset.capturedAtUnixMs
            ))
        }
        return try Self.outcome(TodoExecutionContextResponse(
            title: todo.title,
            detail: todo.detail,
            objective: todo.executionContract.objective,
            scope: todo.executionContract.scope,
            expectedOutputs: todo.executionContract.expectedOutputs,
            acceptanceCriteria: todo.executionContract.acceptanceCriteria,
            constraints: todo.executionContract.constraints,
            priority: todo.priority,
            builtinCapabilities: todo.executionPlan.builtinCapabilities.map(\.rawValue),
            plugins: todo.executionPlan.plugins.map(\.displayName),
            sourceMessages: sourceMessages,
            prerequisites: prerequisites,
            teamAssets: teamAssets
        ))
    }

    private func appendTodoProgress(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        guard let todo = try await currentExecutionTodo() else {
            return Self.structuredFailure(
                code: "todo_execution_context_mismatch",
                field: "delivery",
                message: "当前线程不是有效的 Todo 执行线程。",
                retryable: false
            )
        }
        let progress = try await store.appendAgentTodoProgress(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            kind: .progress,
            runID: context.runID,
            stage: try Self.optionalString(arguments, key: "stage") ?? "",
            detail: try Self.requiredString(arguments, key: "detail"),
            nowUnixMs: now()
        )
        return try Self.outcome(TodoProgressResponse(progress: progress))
    }

    private func readTodoProgress(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let reference = try Self.requiredString(arguments, key: "todo_ref")
        guard let authority = await references.todoAuthority(reference: reference),
              try await isTeamMember(teamRoomID: authority.teamRoomID) else {
            return Self.structuredFailure(
                code: "invalid_todo_ref",
                field: "todo_ref",
                message: "Todo 引用无效或已经过期，请重新调用 todo_list。",
                retryable: true,
                nextTool: Self.todoListToolName
            )
        }
        let limit = Int(try Self.optionalInteger(arguments, key: "limit") ?? 100)
        let progress = try await store.listAgentTodoProgress(
            ownerUserID: context.ownerUserID,
            agentID: authority.agentID,
            todoID: authority.todoID,
            limit: limit
        )
        return try Self.outcome(progress.map(TodoProgressResponse.init(progress:)))
    }

    private func completeTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let summary = try Self.requiredString(arguments, key: "summary")
        return try await finishExecutionTodo(
            status: .completed,
            progressKind: .completed,
            detail: summary,
            blockedReason: "",
            result: summary
        )
    }

    private func blockTodo(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let reason = try Self.requiredString(arguments, key: "reason")
        return try await finishExecutionTodo(
            status: .blocked,
            progressKind: .blocked,
            detail: reason,
            blockedReason: reason,
            result: ""
        )
    }

    private func finishExecutionTodo(
        status: LocalAgentTodoStatus,
        progressKind: LocalAgentTodoProgressKind,
        detail: String,
        blockedReason: String,
        result: String
    ) async throws -> AgentToolOutcome {
        guard let todo = try await currentExecutionTodo() else {
            return Self.structuredFailure(
                code: "todo_execution_context_mismatch",
                field: "delivery",
                message: "当前线程不是有效的 Todo 执行线程。",
                retryable: false
            )
        }
        let timestamp = now()
        _ = try await store.appendAgentTodoProgress(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            kind: progressKind,
            runID: context.runID,
            stage: status == .completed ? "completed" : "blocked",
            detail: detail,
            nowUnixMs: timestamp
        )
        let updated = try await store.updateAgentTodo(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            update: .init(
                status: status,
                blockedReason: blockedReason,
                result: result
            ),
            nowUnixMs: timestamp
        )
        _ = try await store.completeHeartbeatDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID,
            nowUnixMs: timestamp
        )
        _ = try await store.enqueueAgentTodoStatus(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID,
            todoID: todo.id,
            excludingAgentID: nil,
            nowUnixMs: timestamp
        )
        if status == .completed {
            _ = try await store.enqueueReadyDependentAgentTodos(
                ownerUserID: context.ownerUserID,
                prerequisiteTodoID: todo.id,
                nowUnixMs: timestamp
            )
        }
        return try Self.outcome(try await todoResponse(updated))
    }

    private func currentExecutionTodo() async throws -> LocalAgentTodo? {
        guard let delivery = try await store.delivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), delivery.status == .running, delivery.lane == .executor,
        let todo = try await store.todoForDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), todo.agentID == context.agentID, todo.status == .inProgress,
        todo.teamRoomID == context.roomID else { return nil }
        return todo
    }

    private func todoResponses(includeTerminal: Bool) async throws -> [TodoResponse] {
        let rooms = try await store.listRooms(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        var response: [TodoResponse] = []
        for room in rooms where room.conversationKind == .projectTeam {
            guard try await isTeamMember(teamRoomID: room.id) else { continue }
            for todo in try await store.listTeamTodos(
                ownerUserID: context.ownerUserID,
                teamRoomID: room.id,
                includeTerminal: includeTerminal
            ) {
                response.append(try await todoResponse(todo))
            }
        }
        response.sort {
            if $0.priority != $1.priority { return $0.priority > $1.priority }
            return $0.updatedAtUnixMs > $1.updatedAtUnixMs
        }
        return response
    }

    private func todoResponse(_ todo: LocalAgentTodo) async throws -> TodoResponse {
        let todoReference = await references.todoReference(
            todoID: todo.id,
            agentID: todo.agentID,
            teamRoomID: todo.teamRoomID
        )
        let team = try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: todo.teamRoomID
        )
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let names = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0.draft.name) })
        var sourceReferences: [TodoSourceReferenceResponse] = []
        for source in try await store.listAgentTodoSources(
            ownerUserID: context.ownerUserID,
            agentID: todo.agentID,
            todoID: todo.id
        ) {
            sourceReferences.append(.init(
                conversationReference: await references.conversationReference(
                    roomID: source.conversationID
                ),
                messageReference: await references.messageReference(
                    roomID: source.conversationID,
                    messageID: source.messageID
                ),
                relation: source.relation.rawValue
            ))
        }
        var dependencies: [TodoDependencyResponse] = []
        for dependency in try await store.listAgentTodoDependencies(
            ownerUserID: context.ownerUserID,
            agentID: todo.agentID,
            todoID: todo.id
        ) {
            guard let prerequisite = try await store.agentTodo(
                ownerUserID: context.ownerUserID,
                agentID: dependency.prerequisiteAgentID,
                todoID: dependency.prerequisiteTodoID
            ) else { continue }
            dependencies.append(.init(
                todoReference: await references.todoReference(
                    todoID: prerequisite.id,
                    agentID: prerequisite.agentID,
                    teamRoomID: prerequisite.teamRoomID
                ),
                title: prerequisite.title,
                assignee: names[prerequisite.agentID] ?? "Agent",
                status: prerequisite.status.rawValue,
                blockedReason: prerequisite.blockedReason,
                result: prerequisite.result
            ))
        }
        return .init(
            todoReference: todoReference,
            team: team?.draft.name ?? "Team",
            assignee: names[todo.agentID] ?? "Agent",
            assignedToCurrentAgent: todo.agentID == context.agentID,
            title: todo.title,
            detail: todo.detail,
            objective: todo.executionContract.objective,
            scope: todo.executionContract.scope,
            expectedOutputs: todo.executionContract.expectedOutputs,
            acceptanceCriteria: todo.executionContract.acceptanceCriteria,
            constraints: todo.executionContract.constraints,
            priority: todo.priority,
            status: todo.status.rawValue,
            blockedReason: todo.blockedReason,
            result: todo.result,
            builtinCapabilities: todo.executionPlan.builtinCapabilities.map(\.rawValue),
            plugins: todo.executionPlan.plugins.map(\.displayName),
            sources: sourceReferences,
            dependencies: dependencies,
            updatedAtUnixMs: todo.updatedAtUnixMs
        )
    }

    private func readMessages(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let beforeReference = try Self.optionalString(arguments, key: "before_message_ref")
        let beforeMessageID: String?
        if let beforeReference {
            guard let authority = await references.messageAuthority(reference: beforeReference),
                  authority.roomID == context.roomID else {
                return Self.structuredFailure(
                    code: "invalid_message_ref",
                    field: "before_message_ref",
                    message: "消息游标无效或已经过期，请从最近一页重新读取。",
                    retryable: true,
                    nextTool: Self.readMessagesToolName
                )
            }
            beforeMessageID = authority.messageID
        } else {
            beforeMessageID = nil
        }
        let limit = try Self.optionalInteger(arguments, key: "limit").map(Int.init) ?? 50
        let page = try await store.pageRecentMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            beforeMessageID: beforeMessageID,
            limit: limit
        )
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let profilesByID = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0) })
        var messages: [MessageResponse] = []
        for message in page.messages {
            messages.append(await messageResponse(message, profiles: profilesByID))
        }
        let nextReference: String?
        if page.hasMore, let messageID = page.nextCursorMessageID {
            nextReference = await references.messageReference(
                roomID: context.roomID,
                messageID: messageID
            )
        } else { nextReference = nil }
        return try Self.outcome(MessagePageResponse(
            messages: messages,
            nextCursorReference: nextReference,
            hasMore: page.hasMore,
            readThroughReference: nil
        ))
    }

    private func readAttachment(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let messageReference = try Self.requiredString(arguments, key: "message_ref")
        let attachmentReference = try Self.requiredString(arguments, key: "attachment_ref")
        guard let messageAuthority = await references.messageAuthority(
            reference: messageReference
        ), let attachmentAuthority = await references.attachmentAuthority(
            reference: attachmentReference
        ),
        attachmentAuthority.roomID == messageAuthority.roomID,
        attachmentAuthority.messageID == messageAuthority.messageID else {
            return Self.structuredFailure(
                code: "invalid_attachment_ref",
                field: "attachment_ref",
                message: "附件引用无效、已经过期或不属于所选消息，请重新读取消息。",
                retryable: true,
                nextTool: Self.readMessagesToolName
            )
        }
        let messageID = messageAuthority.messageID
        let attachmentID = attachmentAuthority.attachmentID
        let offset = max(0, Int(try Self.optionalInteger(arguments, key: "offset") ?? 0))
        let limit = min(
            12_000,
            max(1, Int(try Self.optionalInteger(arguments, key: "limit") ?? 12_000))
        )
        guard let payload = try await store.messageAttachment(
            ownerUserID: context.ownerUserID,
            roomID: messageAuthority.roomID,
            messageID: messageID,
            attachmentID: attachmentID
        ) else { throw AgentGroupChatError.notFound }
        let data = try Data(contentsOf: payload.localFileURL, options: [.mappedIfSafe])
        var response: [String: NativeJSONValue] = [
            "message_ref": .string(messageReference),
            "attachment_ref": .string(attachmentReference),
            "name": .string(payload.attachment.name),
            "mime_type": .string(payload.attachment.mimeType),
            "kind": .string(payload.attachment.kind.rawValue),
            "size": .number(Double(payload.attachment.size)),
        ]
        if !data.prefix(8_000).contains(0), let text = String(data: data, encoding: .utf8) {
            let characters = Array(text)
            let start = min(offset, characters.count)
            let end = min(start + limit, characters.count)
            response["content"] = .string(String(characters[start..<end]))
            response["offset"] = .number(Double(start))
            response["next_offset"] = end < characters.count ? .number(Double(end)) : .null
            response["has_more"] = .bool(end < characters.count)
        } else {
            response["content"] = .null
            response["multimodal_on_trigger"] = .bool(messageID == context.triggerMessageID)
            response["note"] = .string(
                messageID == context.triggerMessageID
                    ? "该二进制附件已作为当前触发消息的多模态输入提供给模型。"
                    : "该二进制附件不能作为文本读取；请让 Human 在新消息中重新附带，或使用匹配的本机 Plugin。"
            )
        }
        return try Self.outcome(response)
    }

    private func markRead(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let throughReference = try Self.requiredString(arguments, key: "through_message_ref")
        guard let authority = await references.messageAuthority(reference: throughReference),
              authority.roomID == context.roomID else {
            return Self.structuredFailure(
                code: "invalid_message_ref",
                field: "through_message_ref",
                message: "已读消息引用无效或已经过期，请重新读取当前会话未读。",
                retryable: true,
                nextTool: Self.readUnreadToolName
            )
        }
        let cursor = try await store.markMessagesRead(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            throughMessageID: authority.messageID,
            nowUnixMs: now()
        )
        let remaining = try await store.listUnreadMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            limit: 1
        )
        let nextUnreadMessageReference: String?
        if let messageID = remaining.messages.first?.id {
            nextUnreadMessageReference = await references.messageReference(
                roomID: context.roomID,
                messageID: messageID
            )
        } else { nextUnreadMessageReference = nil }
        return try Self.outcome(MarkReadResponse(
            throughMessageReference: await references.messageReference(
                roomID: context.roomID,
                messageID: cursor.messageID
            ),
            hasUnread: !remaining.messages.isEmpty,
            nextUnreadMessageReference: nextUnreadMessageReference
        ))
    }

    private func openDirect(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let targetReference = try Self.requiredString(arguments, key: "target_agent_ref")
        guard let targetAgentID = await references.agentID(reference: targetReference) else {
            return Self.structuredFailure(
                code: "invalid_agent_ref",
                field: "target_agent_ref",
                message: "Agent 引用无效或已经过期，请重新读取账户 Agent 与团队快照。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        let conversation = try await store.openAgentDirect(
            ownerUserID: context.ownerUserID,
            initiatingAgentID: context.agentID,
            targetAgentID: targetAgentID
        )
        await roomChangeHandler(conversation.id)
        return try Self.outcome(DirectOpenResponse(
            conversationReference: await references.conversationReference(roomID: conversation.id),
            targetAgentReference: targetReference
        ))
    }

    private func sendDirect(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let conversationReference = try Self.requiredString(arguments, key: "conversation_ref")
        guard let conversationID = await references.roomID(
            conversationReference: conversationReference
        ) else {
            return Self.structuredFailure(
                code: "invalid_conversation_ref",
                field: "conversation_ref",
                message: "私聊引用无效或已经过期，请重新打开 Agent 私聊。",
                retryable: true,
                nextTool: Self.openDirectToolName
            )
        }
        let content = try Self.requiredString(arguments, key: "content")
        guard let conversation = try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: conversationID
        ), conversation.conversationKind == .agentAgentDirect else {
            throw AgentGroupChatError.notFound
        }
        let members = try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: conversationID
        )
        guard members.contains(where: { $0.agentID == context.agentID }) else {
            throw AgentGroupChatError.notMember
        }
        let post = try await store.postMessage(
            ownerUserID: context.ownerUserID,
            roomID: conversationID,
            draft: .init(
                senderKind: .agent,
                senderID: context.agentID,
                content: content,
                sourceRunID: context.runID,
                causationID: context.deliveryID,
                hopCount: context.hopCount + 1
            ),
            limits: limits
        )
        await roomChangeHandler(conversationID)
        return try Self.outcome(DirectSendResponse(
            conversationReference: conversationReference,
            messageReference: await references.messageReference(
                roomID: conversationID,
                messageID: post.message.id
            ),
            spawnedDeliveryCount: post.deliveries.count,
            routingStopReason: post.routingStopReason
        ))
    }

    private func sendTeam(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let teamReference = try Self.requiredString(arguments, key: "team_ref")
        guard let teamRoomID = await references.teamID(reference: teamReference),
              let team = try await store.room(
                ownerUserID: context.ownerUserID,
                roomID: teamRoomID
              ), team.status == .active,
              team.conversationKind == .projectTeam else {
            return Self.structuredFailure(
                code: "invalid_team_ref",
                field: "team_ref",
                message: "团队引用无效或已经过期，请重新读取账户 Agent 与团队快照。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        let members = try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: teamRoomID
        )
        let activeMemberIDs = Set(members.filter { $0.status == .active }.map(\.agentID))
        guard activeMemberIDs.contains(context.agentID) else {
            throw AgentGroupChatError.notMember
        }
        let mentionReferences = try Self.optionalStringArray(arguments, key: "mention_agent_refs")
        var mentionAgentIDs: [String] = []
        for (index, reference) in mentionReferences.enumerated() {
            guard let agentID = await references.agentID(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_agent_ref",
                    field: "mention_agent_refs[\(index)]",
                    message: "被 @ 的 Agent 引用无效或已经过期，请重新读取账户 Agent 与团队快照。",
                    retryable: true,
                    nextTool: Self.workspaceSnapshotToolName
                )
            }
            guard activeMemberIDs.contains(agentID) else {
                return Self.structuredFailure(
                    code: "agent_not_in_team",
                    field: "mention_agent_refs[\(index)]",
                    message: "被 @ 的 Agent 不是该团队的活跃成员，请重新选择团队成员。",
                    retryable: true,
                    nextTool: Self.workspaceSnapshotToolName
                )
            }
            if agentID != context.agentID, !mentionAgentIDs.contains(agentID) {
                mentionAgentIDs.append(agentID)
            }
        }
        let post = try await store.postMessage(
            ownerUserID: context.ownerUserID,
            roomID: teamRoomID,
            draft: .init(
                senderKind: .agent,
                senderID: context.agentID,
                content: try Self.requiredString(arguments, key: "content"),
                mentionedAgentIDs: mentionAgentIDs,
                sourceRunID: context.runID,
                causationID: context.deliveryID,
                hopCount: context.hopCount + 1
            ),
            limits: limits
        )
        await roomChangeHandler(teamRoomID)
        return try Self.outcome(
            SendResponse(
                messageReference: await references.messageReference(
                    roomID: teamRoomID,
                    messageID: post.message.id
                ),
                completed: false,
                spawnedDeliveryCount: post.deliveries.count,
                routingStopReason: post.routingStopReason
            )
        )
    }

    private func sendMessage(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let content = try Self.requiredString(arguments, key: "content")
        let mentionReferences = try Self.optionalStringArray(arguments, key: "mention_agent_refs")
        var mentionAgentIDs: [String] = []
        for (index, reference) in mentionReferences.enumerated() {
            guard let agentID = await references.agentID(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_agent_ref",
                    field: "mention_agent_refs[\(index)]",
                    message: "被 @ 的 Agent 引用无效或已经过期，请重新读取当前会话成员。",
                    retryable: true,
                    nextTool: Self.listMembersToolName
                )
            }
            mentionAgentIDs.append(agentID)
        }
        let replyToMessageID: String
        if let replyReference = try Self.optionalString(arguments, key: "reply_to_message_ref") {
            guard let authority = await references.messageAuthority(reference: replyReference),
                  authority.roomID == context.roomID else {
                return Self.structuredFailure(
                    code: "invalid_message_ref",
                    field: "reply_to_message_ref",
                    message: "回复消息引用无效或已经过期，请重新读取当前会话消息。",
                    retryable: true,
                    nextTool: Self.readMessagesToolName
                )
            }
            replyToMessageID = authority.messageID
        } else {
            replyToMessageID = context.triggerMessageID
        }
        guard let delivery = try await store.delivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), delivery.status == .running,
           delivery.roomID == context.roomID,
           delivery.targetAgentID == context.agentID,
           delivery.messageID == context.triggerMessageID,
           delivery.rootMessageID == context.rootMessageID else {
            throw AgentGroupChatError.conflict
        }
        let post = try await store.postMessage(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            draft: .init(
                senderKind: .agent,
                senderID: context.agentID,
                content: content,
                mentionedAgentIDs: mentionAgentIDs,
                replyToMessageID: replyToMessageID,
                sourceRunID: context.runID,
                causationID: context.deliveryID,
                rootMessageID: context.rootMessageID,
                hopCount: context.hopCount + 1
            ),
            limits: limits
        )
        await roomChangeHandler(context.roomID)
        // A substantive reply acknowledges the triggering message. This best-effort cursor update
        // is intentionally secondary to the durable message transaction. Sending no longer ends
        // a manager cycle; the Agent must still inspect scheduling state and call cycle_complete.
        _ = try? await store.markMessagesRead(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            throughMessageID: context.triggerMessageID,
            nowUnixMs: now()
        )
        return try Self.outcome(
            SendResponse(
                messageReference: await references.messageReference(
                    roomID: context.roomID,
                    messageID: post.message.id
                ),
                completed: false,
                spawnedDeliveryCount: post.deliveries.count,
                routingStopReason: post.routingStopReason
            )
        )
    }

    private func completeHeartbeat(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        guard let delivery = try await store.delivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), delivery.status == .running,
           delivery.triggerKind == .heartbeat,
           delivery.roomID == context.roomID,
           delivery.targetAgentID == context.agentID else {
            throw AgentGroupChatError.conflict
        }
        _ = try await store.completeHeartbeatDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID,
            nowUnixMs: now()
        )
        return try Self.outcome(["status": "completed"])
    }

    private func completeManagerCycle(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        _ = try Self.arguments(call)
        guard let delivery = try await store.delivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), delivery.status == .running, delivery.lane == .manager,
        delivery.roomID == context.roomID,
        delivery.targetAgentID == context.agentID else {
            throw AgentGroupChatError.conflict
        }
        let scheduling = try await store.agentTodoScheduleState(
            ownerUserID: context.ownerUserID,
            agentID: context.agentID
        )
        if scheduling.runningTodo == nil, scheduling.readyTodo != nil {
            return Self.structuredFailure(
                code: "ready_todo_requires_start",
                field: "manager_cycle",
                message: "当前 Agent 没有执行中的任务，但存在已经 ready 的任务。请先调用 todo_start_next，再结束本轮通讯处理。",
                retryable: true,
                nextTool: Self.todoStartNextToolName
            )
        }
        _ = try await store.completeHeartbeatDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID,
            nowUnixMs: now()
        )
        return try Self.outcome(["status": "completed"])
    }

    private func proposeMember(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard try await canManageStaff() else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        guard let currentProfile = profiles.first(where: { $0.id == context.agentID }) else {
            throw AgentGroupChatError.notFound
        }
        let requestedThinkingLevel = try Self.optionalString(
            arguments,
            key: "thinking_level"
        )?.trimmingCharacters(in: .whitespacesAndNewlines)
        let thinkingLevel = requestedThinkingLevel.flatMap { $0.isEmpty ? nil : $0 }
            ?? currentProfile.draft.thinkingLevel
        let draft = LocalAgentDraft(
            name: try Self.requiredString(arguments, key: "name"),
            role: try Self.requiredString(arguments, key: "role"),
            responsibility: try Self.optionalString(arguments, key: "responsibility") ?? "",
            rolePrompt: try Self.requiredString(arguments, key: "role_prompt"),
            modelConfigID: currentProfile.draft.modelConfigID,
            thinkingLevel: thinkingLevel,
            professionKey: try Self.requiredString(arguments, key: "profession_key"),
            rationale: try Self.optionalString(arguments, key: "rationale") ?? ""
        )
        try draft.validate()
        let proposal = try await store.createAgentProposal(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            proposerAgentID: context.agentID,
            sourceDeliveryID: context.deliveryID,
            requestKey: call.id,
            draft: draft,
            nowUnixMs: now()
        )
        return try Self.outcome(ProposalAcknowledgement(
            type: "agent_creation",
            status: proposal.status.rawValue,
            subject: proposal.draft.name
        ))
    }

    private func proposeMemberRemoval(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard try await canManageStaff() else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let targetReference = try Self.requiredString(arguments, key: "target_agent_ref")
        guard let targetAgentID = await references.agentID(reference: targetReference) else {
            return Self.structuredFailure(
                code: "invalid_agent_ref",
                field: "target_agent_ref",
                message: "Agent 引用无效或已经过期，请重新读取当前会话成员。",
                retryable: true,
                nextTool: Self.listMembersToolName
            )
        }
        let proposal = try await store.createAgentRemovalProposal(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            proposerAgentID: context.agentID,
            sourceDeliveryID: context.deliveryID,
            requestKey: call.id,
            draft: .init(
                targetAgentID: targetAgentID,
                reason: try Self.requiredString(arguments, key: "reason"),
                handoffPlan: try Self.optionalString(arguments, key: "handoff_plan") ?? ""
            ),
            nowUnixMs: now()
        )
        return try Self.outcome(ProposalAcknowledgement(
            type: "agent_removal",
            status: proposal.status.rawValue,
            subject: "团队成员移出提案"
        ))
    }

    private func proposeExistingMember(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard try await canManageStaff() else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let teamReference = try Self.requiredString(arguments, key: "team_ref")
        guard let targetTeamRoomID = await references.teamID(reference: teamReference) else {
            return Self.structuredFailure(
                code: "invalid_team_ref",
                field: "team_ref",
                message: "团队引用无效或已经过期，请重新读取工作区快照。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        let agentReference = try Self.requiredString(arguments, key: "target_agent_ref")
        guard let targetAgentID = await references.agentID(reference: agentReference) else {
            return Self.structuredFailure(
                code: "invalid_agent_ref",
                field: "target_agent_ref",
                message: "Agent 引用无效或已经过期，请重新读取工作区快照。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        do {
            let proposal = try await store.createMembershipProposal(
                ownerUserID: context.ownerUserID,
                sourceRoomID: context.roomID,
                proposerAgentID: context.agentID,
                sourceDeliveryID: context.deliveryID,
                requestKey: call.id,
                draft: .init(
                    targetTeamRoomID: targetTeamRoomID,
                    targetAgentID: targetAgentID,
                    role: try Self.requiredString(arguments, key: "role"),
                    responsibility: try Self.optionalString(
                        arguments,
                        key: "responsibility"
                    ) ?? ""
                ),
                nowUnixMs: now()
            )
            return try Self.outcome(ProposalAcknowledgement(
                type: "existing_agent_membership",
                status: proposal.status.rawValue,
                subject: "现有 Agent 入队提案"
            ))
        } catch let error as AgentGroupChatError {
            let message = switch error {
            case .conflict: "该 Agent 已经是目标团队成员，或相同提案已被处理。"
            case .notFound: "目标团队或 Agent 已不存在，请重新读取工作区快照。"
            case .permissionDenied: "当前 Agent 没有人员管理权限，或本次运行身份已失效。"
            case .invalidField(let field): "邀请参数不符合要求：\(field)。"
            case .storage: "本地成员提案暂时无法保存。"
            case .notMember: "当前 Agent 已不在发起会话中。"
            }
            return Self.structuredFailure(
                code: Self.errorCode(error),
                field: Self.errorField(error),
                message: message,
                retryable: error == .notFound,
                nextTool: error == .notFound ? Self.workspaceSnapshotToolName : nil
            )
        }
    }

    private func canManageStaff() async throws -> Bool {
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        guard let current = profiles.first(where: { $0.id == context.agentID }) else {
            throw AgentGroupChatError.notFound
        }
        return LocalAgentPermission.canManageStaff(current.draft.defaultSkillIDs)
    }

    private func managesAnyProjectTeam() async throws -> Bool {
        try await store.listRooms(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        ).contains {
            $0.conversationKind == .projectTeam
                && $0.projectManagerAgentID == context.agentID
        }
    }

    private func isProjectManager(teamRoomID: String) async throws -> Bool {
        guard let room = try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: teamRoomID
        ), room.status == .active, room.conversationKind == .projectTeam,
        room.projectManagerAgentID == context.agentID else { return false }
        return try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: teamRoomID
        ).contains { $0.agentID == context.agentID && $0.status == .active }
    }

    private func isTeamMember(teamRoomID: String) async throws -> Bool {
        try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: teamRoomID
        ).contains { $0.agentID == context.agentID && $0.status == .active }
    }

    private struct MemberResponse: Encodable {
        let agentReference: String
        let name: String
        let role: String
        let responsibility: String
        let isProjectManager: Bool

        enum CodingKeys: String, CodingKey {
            case agentReference = "agent_ref"
            case name, role, responsibility
            case isProjectManager = "is_project_manager"
        }
    }

    private struct BootstrapResponse: Encodable {
        let agent: MemberResponse
        let conversationReference: String
        let roomName: String
        let roomGoal: String
        let trigger: MessageResponse
        let unread: MessagePageResponse
        let members: [MemberResponse]

        enum CodingKeys: String, CodingKey {
            case agent
            case conversationReference = "conversation_ref"
            case roomName = "room_name"
            case roomGoal = "room_goal"
            case trigger, unread, members
        }
    }

    private struct MarkReadResponse: Encodable {
        let throughMessageReference: String
        let hasUnread: Bool
        let nextUnreadMessageReference: String?

        enum CodingKeys: String, CodingKey {
            case throughMessageReference = "through_message_ref"
            case hasUnread = "has_unread"
            case nextUnreadMessageReference = "next_unread_message_ref"
        }
    }

    private struct AttachmentResponse: Encodable {
        let attachmentReference: String
        let name: String
        let mimeType: String
        let size: Int
        let kind: String

        enum CodingKeys: String, CodingKey {
            case attachmentReference = "attachment_ref"
            case name
            case mimeType = "mime_type"
            case size, kind
        }
    }

    private struct MessageResponse: Encodable {
        let messageReference: String
        let sender: String
        let senderAgentReference: String?
        let content: String
        let replyToMessageReference: String?
        let attachments: [AttachmentResponse]
        let createdAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case messageReference = "message_ref"
            case sender
            case senderAgentReference = "sender_agent_ref"
            case content
            case replyToMessageReference = "reply_to_message_ref"
            case attachments
            case createdAtUnixMs = "created_at_unix_ms"
        }
    }

    private struct MessagePageResponse: Encodable {
        let messages: [MessageResponse]
        let nextCursorReference: String?
        let hasMore: Bool
        let readThroughReference: String?

        enum CodingKeys: String, CodingKey {
            case messages
            case nextCursorReference = "next_before_message_ref"
            case hasMore = "has_more"
            case readThroughReference = "read_through_message_ref"
        }
    }

    private struct InboxResponse: Encodable {
        let conversations: [InboxConversationResponse]
        let messageCount: Int
        let markedRead: Bool

        enum CodingKeys: String, CodingKey {
            case conversations
            case messageCount = "message_count"
            case markedRead = "marked_read"
        }
    }

    private struct InboxConversationResponse: Encodable {
        let conversationReference: String
        let name: String
        let kind: String
        let messages: [InboxMessageResponse]

        enum CodingKeys: String, CodingKey {
            case conversationReference = "conversation_ref"
            case name, kind, messages
        }
    }

    private struct InboxMessageResponse: Encodable {
        let messageReference: String
        let sender: String
        let content: String
        let attachments: [AttachmentResponse]
        let createdAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case messageReference = "message_ref"
            case sender, content, attachments
            case createdAtUnixMs = "created_at_unix_ms"
        }
    }

    private struct InboxSendResponse: Encodable {
        let sent: Bool
        let notifiedProjectManager: Bool
        let conversationReference: String
        let replyToMessageReference: String
        let spawnedDeliveryCount: Int
        let routingStopReason: String?

        enum CodingKeys: String, CodingKey {
            case sent
            case notifiedProjectManager = "notified_project_manager"
            case conversationReference = "conversation_ref"
            case replyToMessageReference = "reply_to_message_ref"
            case spawnedDeliveryCount = "spawned_delivery_count"
            case routingStopReason = "routing_stop_reason"
        }
    }

    private struct WorkspaceSnapshotResponse: Encodable {
        let agents: [WorkspaceAgentResponse]
        let teams: [WorkspaceTeamResponse]
    }

    private struct WorkspaceAgentResponse: Encodable {
        let agentReference: String
        let name: String
        let profession: String
        let isCurrentAgent: Bool
        let teams: [String]

        enum CodingKeys: String, CodingKey {
            case agentReference = "agent_ref"
            case name, profession
            case isCurrentAgent = "is_current_agent"
            case teams
        }
    }

    private struct WorkspaceTeamResponse: Encodable {
        let teamReference: String
        let name: String
        let goal: String
        let hasProjectManager: Bool
        let projectManager: String?
        let members: [WorkspaceTeamMemberResponse]

        enum CodingKeys: String, CodingKey {
            case teamReference = "team_ref"
            case name, goal
            case hasProjectManager = "has_project_manager"
            case projectManager = "project_manager"
            case members
        }
    }

    private struct WorkspaceTeamMemberResponse: Encodable {
        let agentReference: String
        let name: String
        let profession: String
        let role: String
        let isProjectManager: Bool

        enum CodingKeys: String, CodingKey {
            case agentReference = "agent_ref"
            case name, profession, role
            case isProjectManager = "is_project_manager"
        }
    }

    private struct TodoResponse: Encodable {
        let todoReference: String
        let team: String
        let assignee: String
        let assignedToCurrentAgent: Bool
        let title: String
        let detail: String
        let objective: String
        let scope: String
        let expectedOutputs: [String]
        let acceptanceCriteria: [String]
        let constraints: [String]
        let priority: Int
        let status: String
        let blockedReason: String
        let result: String
        let builtinCapabilities: [String]
        let plugins: [String]
        let sources: [TodoSourceReferenceResponse]
        let dependencies: [TodoDependencyResponse]
        let updatedAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case todoReference = "todo_ref"
            case team, assignee
            case assignedToCurrentAgent = "assigned_to_current_agent"
            case title, detail, objective, scope, constraints, priority, status
            case expectedOutputs = "expected_outputs"
            case acceptanceCriteria = "acceptance_criteria"
            case blockedReason = "blocked_reason"
            case result
            case builtinCapabilities = "builtin_capabilities"
            case plugins
            case sources, dependencies
            case updatedAtUnixMs = "updated_at_unix_ms"
        }
    }

    private struct TodoScheduleStateResponse: Encodable {
        let state: String
        let runningTodo: TodoResponse?
        let readyTodo: TodoResponse?

        enum CodingKeys: String, CodingKey {
            case state
            case runningTodo = "running_todo"
            case readyTodo = "ready_todo"
        }
    }

    private struct TodoStartNextResponse: Encodable {
        let status: String
        let todo: TodoResponse?
    }

    private struct TeamAssetSummaryResponse: Encodable {
        let assetReference: String
        let category: String
        let title: String
        let revision: Int
        let updatedAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case assetReference = "asset_ref"
            case category, title, revision
            case updatedAtUnixMs = "updated_at_unix_ms"
        }
    }

    private struct TeamAssetDetailResponse: Encodable {
        let assetReference: String
        let category: String
        let title: String
        let markdown: String
        let revision: Int
        let updatedAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case assetReference = "asset_ref"
            case category, title, markdown, revision
            case updatedAtUnixMs = "updated_at_unix_ms"
        }
    }

    private struct TodoDependencyResponse: Encodable {
        let todoReference: String
        let title: String
        let assignee: String
        let status: String
        let blockedReason: String
        let result: String

        enum CodingKeys: String, CodingKey {
            case todoReference = "todo_ref"
            case title, assignee, status
            case blockedReason = "blocked_reason"
            case result
        }
    }

    private struct TodoSourceReferenceResponse: Encodable {
        let conversationReference: String
        let messageReference: String
        let relation: String

        enum CodingKeys: String, CodingKey {
            case conversationReference = "conversation_ref"
            case messageReference = "message_ref"
            case relation
        }
    }

    private struct TodoTeamOptionResponse: Encodable {
        let teamReference: String
        let name: String
        let goal: String
        let assignees: [TodoAssigneeOptionResponse]

        enum CodingKeys: String, CodingKey {
            case teamReference = "team_ref"
            case name, goal, assignees
        }
    }

    private struct TodoAssigneeOptionResponse: Encodable {
        let assigneeReference: String
        let name: String
        let profession: String
        let role: String
        let isProjectManager: Bool

        enum CodingKeys: String, CodingKey {
            case assigneeReference = "assignee_ref"
            case name, profession, role
            case isProjectManager = "is_project_manager"
        }
    }

    private struct TodoDependencyOptionResponse: Encodable {
        let todoReference: String
        let title: String
        let assignee: String
        let status: String
        let blockedReason: String
        let result: String

        enum CodingKeys: String, CodingKey {
            case todoReference = "todo_ref"
            case title, assignee, status
            case blockedReason = "blocked_reason"
            case result
        }
    }

    private struct TodoPluginOptionResponse: Encodable {
        let pluginReference: String
        let name: String
        let description: String

        enum CodingKeys: String, CodingKey {
            case pluginReference = "plugin_ref"
            case name, description
        }
    }

    private struct TodoExecutionOptionsResponse: Encodable {
        let teams: [TodoTeamOptionResponse]
        let builtinCapabilities: [String]
        let plugins: [TodoPluginOptionResponse]

        enum CodingKeys: String, CodingKey {
            case teams
            case builtinCapabilities = "builtin_capabilities"
            case plugins
        }
    }

    private struct TodoSourceMessageResponse: Encodable {
        let relation: String
        let content: String
        let attachmentNames: [String]
        let createdAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case relation, content
            case attachmentNames = "attachment_names"
            case createdAtUnixMs = "created_at_unix_ms"
        }
    }

    private struct TodoExecutionContextResponse: Encodable {
        let title: String
        let detail: String
        let objective: String
        let scope: String
        let expectedOutputs: [String]
        let acceptanceCriteria: [String]
        let constraints: [String]
        let priority: Int
        let builtinCapabilities: [String]
        let plugins: [String]
        let sourceMessages: [TodoSourceMessageResponse]
        let prerequisites: [TodoDependencyResponse]
        let teamAssets: [TeamAssetSummaryResponse]

        enum CodingKeys: String, CodingKey {
            case title, detail, objective, scope, constraints, priority
            case expectedOutputs = "expected_outputs"
            case acceptanceCriteria = "acceptance_criteria"
            case builtinCapabilities = "builtin_capabilities"
            case plugins, prerequisites
            case sourceMessages = "source_messages"
            case teamAssets = "team_assets"
        }
    }

    private struct TodoProgressResponse: Encodable {
        let sequence: Int64
        let kind: String
        let stage: String
        let detail: String
        let createdAtUnixMs: Int64

        init(progress: LocalAgentTodoProgress) {
            sequence = progress.sequence
            kind = progress.kind.rawValue
            stage = progress.stage
            detail = progress.detail
            createdAtUnixMs = progress.createdAtUnixMs
        }

        enum CodingKeys: String, CodingKey {
            case sequence, kind, stage, detail
            case createdAtUnixMs = "created_at_unix_ms"
        }
    }

    private struct StructuredToolFailure: Encodable {
        struct Detail: Encodable {
            let code: String
            let field: String?
            let message: String
            let retryable: Bool
            let nextTool: String?

            enum CodingKeys: String, CodingKey {
                case code, field, message, retryable
                case nextTool = "next_tool"
            }
        }

        let ok = false
        let error: Detail
    }

    private struct ProposalAcknowledgement: Encodable {
        let type: String
        let status: String
        let subject: String
    }

    private struct DirectOpenResponse: Encodable {
        let conversationReference: String
        let targetAgentReference: String

        enum CodingKeys: String, CodingKey {
            case conversationReference = "conversation_ref"
            case targetAgentReference = "target_agent_ref"
        }
    }

    private struct DirectSendResponse: Encodable {
        let conversationReference: String
        let messageReference: String
        let spawnedDeliveryCount: Int
        let routingStopReason: String?

        enum CodingKeys: String, CodingKey {
            case conversationReference = "conversation_ref"
            case messageReference = "message_ref"
            case spawnedDeliveryCount = "spawned_delivery_count"
            case routingStopReason = "routing_stop_reason"
        }
    }

    private struct SendResponse: Encodable {
        let messageReference: String
        let completed: Bool
        let spawnedDeliveryCount: Int
        let routingStopReason: String?

        enum CodingKeys: String, CodingKey {
            case messageReference = "message_ref"
            case completed
            case spawnedDeliveryCount = "spawned_delivery_count"
            case routingStopReason = "routing_stop_reason"
        }
    }

    private static func arguments(_ call: AgentToolCall) throws -> [String: Any] {
        guard let data = call.arguments.data(using: .utf8),
              let value = try? JSONSerialization.jsonObject(with: data),
              let object = value as? [String: Any] else {
            throw AgentGroupChatError.invalidField("arguments")
        }
        return object
    }

    private static func requiredString(_ object: [String: Any], key: String) throws -> String {
        guard let value = object[key] as? String else {
            throw AgentGroupChatError.invalidField(key)
        }
        return value
    }

    private static func optionalString(_ object: [String: Any], key: String) throws -> String? {
        guard let value = object[key] else { return nil }
        guard !(value is NSNull), let string = value as? String else {
            throw AgentGroupChatError.invalidField(key)
        }
        return string
    }

    private static func optionalInteger(_ object: [String: Any], key: String) throws -> Int64? {
        guard let value = object[key] else { return nil }
        guard !(value is NSNull), let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else {
            throw AgentGroupChatError.invalidField(key)
        }
        let integer = number.int64Value
        guard number.doubleValue == Double(integer) else {
            throw AgentGroupChatError.invalidField(key)
        }
        return integer
    }

    private static func optionalBoolean(_ object: [String: Any], key: String) throws -> Bool? {
        guard let value = object[key] else { return nil }
        guard !(value is NSNull), let number = value as? NSNumber,
              CFGetTypeID(number) == CFBooleanGetTypeID() else {
            throw AgentGroupChatError.invalidField(key)
        }
        return number.boolValue
    }

    private static func optionalStringArray(_ object: [String: Any], key: String) throws -> [String] {
        guard let value = object[key] else { return [] }
        guard let values = value as? [String] else {
            throw AgentGroupChatError.invalidField(key)
        }
        return values
    }

    private static func optionalObjectArray(
        _ object: [String: Any],
        key: String
    ) throws -> [[String: Any]] {
        guard let value = object[key] else { return [] }
        guard let values = value as? [[String: Any]] else {
            throw AgentGroupChatError.invalidField(key)
        }
        return values
    }

    private static func outcome<Value: Encodable>(_ value: Value) throws -> AgentToolOutcome {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return .init(String(decoding: try encoder.encode(value), as: UTF8.self))
    }

    private static func structuredFailure(
        code: String,
        field: String?,
        message: String,
        retryable: Bool,
        nextTool: String? = nil
    ) -> AgentToolOutcome {
        let response = StructuredToolFailure(error: .init(
            code: code,
            field: field,
            message: message,
            retryable: retryable,
            nextTool: nextTool
        ))
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let content = (try? encoder.encode(response)).map {
            String(decoding: $0, as: UTF8.self)
        } ?? #"{"ok":false,"error":{"code":"internal_error","message":"工具校验失败。","retryable":false}}"#
        return .failure(content)
    }

    private static func errorCode(_ error: AgentGroupChatError) -> String {
        switch error {
        case .invalidField: "invalid_argument"
        case .notFound: "resource_not_found"
        case .conflict: "state_conflict"
        case .notMember: "agent_not_in_team"
        case .permissionDenied: "permission_denied"
        case .storage: "storage_error"
        }
    }

    private static func errorField(_ error: AgentGroupChatError) -> String? {
        guard case let .invalidField(field) = error else { return nil }
        return field
    }

    private static let toolDefinitions: [AgentToolDefinition] = [
        .init(
            name: bootstrapToolName,
            description: "连接本地 Relay MCP 后读取当前 Agent 身份、绑定项目、团队、成员和本次唤醒消息。身份与范围由客户端固定，不能由参数切换。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: workspaceSnapshotToolName,
            description: "读取当前账户在本机已有的全部活跃 Agent、项目团队、成员关系和显式项目经理。私聊中的 relay_bootstrap 只描述当前会话，不能据此判断其他团队或 Agent 不存在；回答组织现状、既有团队、成员或人员缺口前必须调用本工具。仅返回本轮临时引用，不暴露真实 Agent、团队或项目 ID。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: getTriggerToolName,
            description: "读取唤醒当前 Agent 的群聊消息。项目、房间和消息身份由运行上下文固定。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: listMembersToolName,
            description: "列出当前项目群聊中的 Agent 成员及职责。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: readUnreadToolName,
            description: "读取当前 Agent 在这个群聊中的未读消息。已读位置按 Agent 独立持久化；读取不会自动确认，处理后调用 chat_mark_read。",
            schema: Data(#"{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: readAllUnreadToolName,
            description: "读取当前 Agent 在全部群聊和私聊中的未读消息。返回内容即视为已读并自动推进各会话游标；只返回本轮临时引用，不暴露真实会话、消息或项目 ID。消息不一定需要行动，请自行判断是否回复、忽略或加入 TodoList。",
            schema: Data(#"{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":500}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: inboxSendToolName,
            description: "使用 chat_read_all_unread 本轮返回的临时引用回复原群聊或私聊。普通成员需要把新增工作交给项目经理任务化时，在项目团队会话设置 notify_project_manager=true，由客户端解析并唤醒该团队明确绑定的项目经理。",
            schema: Data(#"{"type":"object","properties":{"conversation_ref":{"type":"string","minLength":1,"maxLength":600},"reply_to_message_ref":{"type":"string","minLength":1,"maxLength":600},"content":{"type":"string","minLength":1,"maxLength":64000},"notify_project_manager":{"type":"boolean","default":false}},"required":["conversation_ref","reply_to_message_ref","content"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: readMessagesToolName,
            description: "从最近一页开始，向更早方向分页读取当前会话记录。需要更早消息时，把响应中的 next_before_message_ref 作为 before_message_ref 继续读取。所有引用只在本轮有效。",
            schema: Data(#"{"type":"object","properties":{"before_message_ref":{"type":"string","minLength":1,"maxLength":600},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: readAttachmentToolName,
            description: "按消息和附件的本轮临时引用读取当前会话附件。文本可用 offset/limit 分段读取；当前触发消息中的图片或 PDF 已由客户端直接作为多模态输入交给模型。",
            schema: Data(#"{"type":"object","properties":{"message_ref":{"type":"string","minLength":1,"maxLength":600},"attachment_ref":{"type":"string","minLength":1,"maxLength":600},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":12000}},"required":["message_ref","attachment_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: markReadToolName,
            description: "把当前 Agent 的独立已读游标推进到指定本轮消息引用。游标单调前进，旧调用或重试不会把已读位置回退。",
            schema: Data(#"{"type":"object","properties":{"through_message_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["through_message_ref"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: openDirectToolName,
            description: "使用 agent_workspace_snapshot 或成员列表返回的临时 Agent 引用打开或复用私聊。不能与自己私聊；A 到 B 和 B 到 A 会得到同一个 conversation_ref。私聊用于一对一补充、敏感事项或非共同团队协作；同一项目团队的启动、分工、依赖、进度、阻塞和交付应优先使用 chat_team_send 在团队群内沟通。",
            schema: Data(#"{"type":"object","properties":{"target_agent_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["target_agent_ref"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: sendDirectToolName,
            description: "向已经打开的 Agent 私聊发送消息。当前 Agent 必须是该私聊参与者，成功后会通过本地 delivery 唤醒对方。不得用多个私聊替代同一项目团队本应公开的协作；项目协作默认使用 chat_team_send。",
            schema: Data(#"{"type":"object","properties":{"conversation_ref":{"type":"string","minLength":1,"maxLength":600},"content":{"type":"string","minLength":1,"maxLength":64000}},"required":["conversation_ref","content"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: sendTeamToolName,
            description: "向 agent_workspace_snapshot 返回的项目团队主动发送一条新群消息，可用同一快照中的 Agent 临时引用精确 @ 团队成员并通过本地 delivery 唤醒他们。当前 Agent 必须是该团队活跃成员，被 @ 的 Agent 也必须属于该团队。项目启动、分工、依赖、进度、阻塞、决策和交付默认使用本工具公开协作；无需唤醒成员的状态同步可不传 mention_agent_refs。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600},"content":{"type":"string","minLength":1,"maxLength":64000},"mention_agent_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":64,"uniqueItems":true}},"required":["team_ref","content"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: Self.proposeMemberToolName,
            description: "使用已授予的人员管理权限，向 Human 提交一个新 Agent 草案。该工具只持久化待确认提案，绝不会直接创建 Agent；私聊中确认后只创建独立 Agent，团队会话中确认后才加入当前团队。模型配置由客户端继承并透传，AI 不填写模型 ID；thinking_level 省略时继承当前 Agent。",
            schema: Data(#"{"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":120},"role":{"type":"string","minLength":1,"maxLength":160},"responsibility":{"type":"string","maxLength":8000},"role_prompt":{"type":"string","minLength":1,"maxLength":32000},"thinking_level":{"type":"string","enum":["auto","none","minimal","low","medium","high","xhigh","max"]},"rationale":{"type":"string","maxLength":4000}},"required":["name","role","role_prompt"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: proposeExistingMemberToolName,
            description: "使用人员管理权限，把账户中已有 Agent 邀请进指定项目团队。先调用 agent_workspace_snapshot，使用其中同一轮返回的 team_ref 和 agent_ref；真实 ID 由客户端解析，不得猜测。该工具只生成待确认提案，Human 确认后才建立成员关系；一个 Agent 可以加入多个团队。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600},"target_agent_ref":{"type":"string","minLength":1,"maxLength":600},"role":{"type":"string","minLength":1,"maxLength":160},"responsibility":{"type":"string","maxLength":8000}},"required":["team_ref","target_agent_ref","role"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: proposeMemberRemovalToolName,
            description: "使用已授予的人员管理权限，向 Human 提交把一个 Agent 移出当前项目团队的提案。必须提供事实理由和可选交接计划；该工具不会删除可复用的 Agent profile，也不会绕过 Human 确认。",
            schema: Data(#"{"type":"object","properties":{"target_agent_ref":{"type":"string","minLength":1,"maxLength":600},"reason":{"type":"string","minLength":1,"maxLength":4000},"handoff_plan":{"type":"string","maxLength":8000}},"required":["target_agent_ref","reason"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: sendMessageToolName,
            description: "以当前 Agent 身份回复当前会话。需要 @ 成员或回复指定消息时，只能使用本轮成员和消息临时引用；发送不会结束通讯周期，仍需检查未读和任务调度并调用 agent_cycle_complete。",
            schema: Data(#"{"type":"object","properties":{"content":{"type":"string","minLength":1,"maxLength":64000},"mention_agent_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":32,"uniqueItems":true},"reply_to_message_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["content"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: completeHeartbeatToolName,
            description: "仅用于主动巡检：当前会话没有需要汇报或执行的事项时，安静完成本次巡检，不向聊天记录发送消息。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: completeManagerCycleToolName,
            description: "结束一次消息、主动巡检或 Todo 状态唤醒的通讯周期。调用前必须完成必要回复和任务调整，并确认有执行中任务、已调用 todo_start_next，或当前没有 ready 任务。不会向聊天记录写入内部状态消息。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoListToolName,
            description: "读取当前 Agent 所属项目团队的共享任务板，并标明团队、负责人、依赖和是否分配给自己。普通成员只能查看；只有团队明确指定的项目经理可以修改。默认不返回已完成或已取消任务。",
            schema: Data(#"{"type":"object","properties":{"include_terminal":{"type":"boolean","default":false}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoScheduleStateToolName,
            description: "读取当前 Agent 的任务调度状态：是否已有执行中的 Todo，以及自己最高优先级且所有前置均已完成的 ready Todo。真实 ID 不会返回。每个通讯周期结束前应调用。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoStartNextToolName,
            description: "由当前 Agent 的通讯线程原子启动自己最高优先级的 ready Todo。若已有 executor 则返回 executor_busy；没有 ready Todo 则返回 no_ready_todo。模型不能指定 Todo ID，因此不能绕过优先级和前置校验。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoExecutionOptionsToolName,
            description: "仅供项目经理读取自己管理的团队、可分配成员、基础能力和本机 Plugin 临时选项。创建 Todo 前必须调用；真实团队、项目、Agent 和 Plugin ID 不会返回。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoDependencyOptionsToolName,
            description: "仅供项目经理读取某个团队可作为前置任务的 Todo 临时引用。创建或更新依赖前调用；只能建立同团队依赖，客户端会拒绝自依赖、重复和环。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["team_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoAddToolName,
            description: "仅供项目经理在共享团队任务板创建 Todo。必须明确目标、范围、交付物、验收条件和约束，并选择负责人、前置任务及可信执行能力。team_ref/assignee_ref/plugin_ref 必须来自 todo_execution_options，真实 ID 由客户端解析和校验。",
            schema: Data(#"{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":500},"detail":{"type":"string","maxLength":16000},"objective":{"type":"string","minLength":1,"maxLength":8000},"scope":{"type":"string","minLength":1,"maxLength":16000},"expected_outputs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"minItems":1,"maxItems":64},"acceptance_criteria":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"minItems":1,"maxItems":64},"constraints":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"maxItems":64},"priority":{"type":"integer","minimum":0,"maximum":100,"default":50},"team_ref":{"type":"string","minLength":1,"maxLength":600},"assignee_ref":{"type":"string","minLength":1,"maxLength":600},"depends_on_todo_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":64,"uniqueItems":true},"source_message_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"minItems":1,"maxItems":64,"uniqueItems":true},"requires_execution":{"type":"boolean","default":true},"builtin_capabilities":{"type":"array","items":{"type":"string","enum":["project_read","project_write","terminal"]},"maxItems":3,"uniqueItems":true},"plugin_hints":{"type":"array","items":{"type":"object","properties":{"plugin_ref":{"type":"string","minLength":1,"maxLength":600},"reason":{"type":"string","maxLength":1000}},"required":["plugin_ref"],"additionalProperties":false},"maxItems":32}},"required":["title","objective","scope","expected_outputs","acceptance_criteria","team_ref","assignee_ref","source_message_refs","requires_execution","builtin_capabilities"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoUpdateToolName,
            description: "仅供项目经理更新团队 Todo 的标题、说明、优先级、前置任务，或将阻塞任务重开、取消过时任务。这里管理的是任务结构和调度状态；负责人即使不是项目经理，仍在独立执行线程中通过 todo_progress_append、todo_block、todo_complete 记录过程并修改自己任务的执行状态。",
            schema: Data(#"{"type":"object","properties":{"todo_ref":{"type":"string","minLength":1,"maxLength":600},"title":{"type":"string","minLength":1,"maxLength":500},"detail":{"type":"string","maxLength":16000},"objective":{"type":"string","minLength":1,"maxLength":8000},"scope":{"type":"string","minLength":1,"maxLength":16000},"expected_outputs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"minItems":1,"maxItems":64},"acceptance_criteria":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"minItems":1,"maxItems":64},"constraints":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"maxItems":64},"priority":{"type":"integer","minimum":0,"maximum":100},"status":{"type":"string","enum":["pending","cancelled"]},"depends_on_todo_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":64,"uniqueItems":true},"source_message_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":64,"uniqueItems":true}},"required":["todo_ref"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoReorderToolName,
            description: "仅供项目经理显式调整同一个团队任务板中未完成 Todo 的优先顺序。数组中靠前的任务优先；未完成前置任务仍不会被调度。",
            schema: Data(#"{"type":"object","properties":{"todo_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"minItems":1,"maxItems":500,"uniqueItems":true}},"required":["todo_refs"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoReadProgressToolName,
            description: "通讯线程读取一个 Todo 执行线程持续写入的阶段、动作、结果或阻塞记录。使用 todo_list 返回的临时 todo_ref。",
            schema: Data(#"{"type":"object","properties":{"todo_ref":{"type":"string","minLength":1,"maxLength":600},"limit":{"type":"integer","minimum":1,"maximum":500,"default":100}},"required":["todo_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: teamAssetListToolName,
            description: "列出项目团队的共享资产目录，并返回本轮临时 asset_ref。Todo 执行线程固定读取当前任务所属团队；通讯线程在私聊中先用 agent_workspace_snapshot 获取 team_ref。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: teamAssetGetToolName,
            description: "读取 team_asset_list 返回的某一版团队共享资产 Markdown。引用包含 revision，资产更新后必须重新列出，禁止猜测真实资产 ID。",
            schema: Data(#"{"type":"object","properties":{"asset_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["asset_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: teamAssetUpsertToolName,
            description: "仅供团队明确指定的项目经理创建或更新团队共享资产。新建时提供 team_ref；更新时提供 asset_ref 和 expected_revision。Markdown 应维护项目背景、进度、技术栈、架构、规范、决策或参考资料。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600},"asset_ref":{"type":"string","minLength":1,"maxLength":600},"category":{"type":"string","enum":["overview","current_progress","tech_stack","architecture","conventions","decision","reference"]},"title":{"type":"string","minLength":1,"maxLength":240},"markdown":{"type":"string","minLength":1,"maxLength":128000},"expected_revision":{"type":"integer","minimum":1}},"required":["category","title","markdown"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: teamAssetArchiveToolName,
            description: "仅供团队明确指定的项目经理归档共享资产。只能使用 team_asset_list 返回的当前 asset_ref，客户端按 revision 防止覆盖并发修改。",
            schema: Data(#"{"type":"object","properties":{"asset_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["asset_ref"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoGetContextToolName,
            description: "仅用于 Todo 执行线程：读取当前 delivery 绑定的任务、可信能力计划和来源消息，不接受任何 ID 参数。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoProgressAppendToolName,
            description: "仅用于 Todo 执行线程：当前任务负责人记录阶段、已执行动作、观察结果和下一步，使通讯线程可以随时查看进度；不要求负责人是项目经理。",
            schema: Data(#"{"type":"object","properties":{"stage":{"type":"string","maxLength":240},"detail":{"type":"string","minLength":1,"maxLength":16000}},"required":["detail"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoCompleteToolName,
            description: "仅用于 Todo 执行线程：当前任务负责人保存完成总结，把自己负责的任务置为 completed 并结束执行；不要求负责人是项目经理。客户端同时记录完成事件。",
            schema: Data(#"{"type":"object","properties":{"summary":{"type":"string","minLength":1,"maxLength":16000}},"required":["summary"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoBlockToolName,
            description: "仅用于 Todo 执行线程：当前任务负责人保存阻塞原因和执行现场，把自己负责的任务置为 blocked 并结束执行；不要求负责人是项目经理。随后由通讯线程向来源会话沟通。",
            schema: Data(#"{"type":"object","properties":{"reason":{"type":"string","minLength":1,"maxLength":8000}},"required":["reason"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
    ]

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
}
