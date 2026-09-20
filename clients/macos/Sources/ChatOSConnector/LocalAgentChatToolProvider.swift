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
    public static let teamAssetUpsertToolName = "team_asset_upsert"
    public static let teamAssetArchiveToolName = "team_asset_archive"

    let store: any AgentGroupChatStore
    let context: LocalAgentChatRunContext
    let professions: [LocalAgentProfessionDefinition]
    let limits: AgentGroupChatRoutingLimits
    let now: @Sendable () -> Int64
    let references: LocalAgentRunReferenceVault
    let todoPluginOptions: [LocalAgentTodoPluginOption]
    let todoCancellationHandler: @Sendable (String) async -> Void
    let roomChangeHandler: @Sendable (String) async -> Void

    public init(
        store: any AgentGroupChatStore,
        context: LocalAgentChatRunContext,
        professions: [LocalAgentProfessionDefinition] = LocalAgentSkillCatalog.professions,
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
        self.limits = limits
        self.now = now
        self.references = LocalAgentRunReferenceVault(
            documentDraftDirectoryURL: documentDraftDirectoryURL
        )
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
        if let replayed = await replayedSendOutcome(call) { return replayed }
        let arguments = try Self.arguments(call)
        let content = try Self.requiredString(arguments, key: "content")
        let lengthFailure = Self.messageLengthFailure(content)
        await recordMessageAttempt(content, rejected: lengthFailure != nil)
        if let lengthFailure { return lengthFailure }
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
        let resolution = try await resolveDocumentDrafts(arguments: arguments, callID: call.id)
        guard case let .ready(documentReferences, attachmentDrafts) = resolution else {
            if case let .failure(failure) = resolution { return failure }
            fatalError("unreachable document resolution")
        }
        let post: AgentGroupChatPostResult
        do {
            post = try await store.postMessage(
                ownerUserID: context.ownerUserID,
                roomID: roomID,
                draft: .init(
                    senderKind: .agent,
                    senderID: context.agentID,
                    content: content,
                    mentionedAgentIDs: mentionedAgentIDs,
                    replyToMessageID: replyMessage.id,
                    sourceRunID: context.runID,
                    causationID: context.deliveryID,
                    rootMessageID: replyMessage.rootMessageID,
                    hopCount: min(64, replyMessage.hopCount + 1),
                    attachments: attachmentDrafts
                ),
                limits: limits
            )
        } catch {
            await references.releaseDocuments(references: documentReferences, callID: call.id)
            throw error
        }
        await references.consumeDocuments(references: documentReferences, callID: call.id)
        await roomChangeHandler(roomID)
        let outcome = try Self.outcome(InboxSendResponse(
            sent: true,
            notifiedProjectManager: notifyProjectManager,
            conversationReference: conversationReference,
            replyToMessageReference: replyReference,
            spawnedDeliveryCount: post.deliveries.count,
            routingStopReason: post.routingStopReason
        ))
        await recordSendOutcome(outcome, call: call)
        return outcome
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

    private func createDocument(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let rawName = try Self.requiredString(arguments, key: "name")
        let title = try Self.requiredString(arguments, key: "title")
        let markdown = try Self.requiredString(arguments, key: "markdown")
        guard let name = Self.sanitizedMarkdownDocumentName(rawName) else {
            await recordDocumentCreation(.invalidName)
            return Self.structuredFailure(
                code: "invalid_document_name",
                field: "name",
                message: "文档名称不能为空；客户端会自动清洗路径字符并补充 .md 后缀。",
                retryable: true,
                nextTool: Self.createDocumentToolName
            )
        }
        guard !title.isEmpty,
              title == title.trimmingCharacters(in: .whitespacesAndNewlines),
              title.count <= 512,
              title.rangeOfCharacter(from: .controlCharacters) == nil else {
            await recordDocumentCreation(.invalidTitle)
            return Self.structuredFailure(
                code: "invalid_document_title",
                field: "title",
                message: "文档标题必须是 1～512 个字符，且不能包含控制字符或首尾空白。",
                retryable: true,
                nextTool: Self.createDocumentToolName
            )
        }
        guard !markdown.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            await recordDocumentCreation(.empty)
            return Self.structuredFailure(
                code: "empty_document",
                field: "markdown",
                message: "Markdown 文档不能为空。",
                retryable: true,
                nextTool: Self.createDocumentToolName
            )
        }
        let data = Data(markdown.utf8)
        do {
            let created = try await references.createDocument(
                name: name,
                title: title,
                data: data
            )
            await recordDocumentCreation(.succeeded, bytes: created.size)
            return try Self.outcome(DocumentCreateResponse(
                documentReference: created.reference,
                name: name,
                title: title,
                size: created.size,
                mimeType: "text/markdown",
                sha256: created.sha256,
                instruction: "请在下一次发送消息时通过 document_refs 附加该文档。"
            ))
        } catch let failure as LocalAgentRunReferenceVault.DocumentCreateFailure {
            switch failure {
            case .empty:
                await recordDocumentCreation(.empty)
                return Self.structuredFailure(
                    code: "empty_document",
                    field: "markdown",
                    message: "Markdown 文档不能为空。",
                    retryable: true,
                    nextTool: Self.createDocumentToolName
                )
            case .tooLarge:
                await recordDocumentCreation(.tooLarge, bytes: data.count)
                return Self.structuredFailure(
                    code: "document_too_large",
                    field: "markdown",
                    message: "单个文档超过 \(AgentCommunicationPolicy.standard.maximumDocumentBytes) 字节，请拆分为少量有意义的 Markdown 文档。",
                    retryable: true,
                    nextTool: Self.createDocumentToolName
                )
            case .tooMany:
                await recordDocumentCreation(.tooMany, bytes: data.count)
                return Self.structuredFailure(
                    code: "too_many_documents",
                    field: nil,
                    message: "当前 Run 已达到最多 \(AgentCommunicationPolicy.standard.maximumDocumentsPerRun) 个文档。",
                    retryable: false
                )
            case .runTooLarge:
                await recordDocumentCreation(.runLimitExceeded, bytes: data.count)
                return Self.structuredFailure(
                    code: "document_run_limit_exceeded",
                    field: "markdown",
                    message: "当前 Run 创建的文档总量超过 \(AgentCommunicationPolicy.standard.maximumDocumentBytesPerRun) 字节。",
                    retryable: false
                )
            case .storage:
                await recordDocumentCreation(.storageFailed, bytes: data.count)
                return Self.structuredFailure(
                    code: "document_storage_failed",
                    field: nil,
                    message: "客户端无法安全保存本地文档草稿。",
                    retryable: true,
                    nextTool: Self.createDocumentToolName
                )
            }
        }
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
        if let replayed = await replayedSendOutcome(call) { return replayed }
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
        let lengthFailure = Self.messageLengthFailure(content)
        await recordMessageAttempt(content, rejected: lengthFailure != nil)
        if let lengthFailure { return lengthFailure }
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
        let resolution = try await resolveDocumentDrafts(arguments: arguments, callID: call.id)
        guard case let .ready(documentReferences, attachmentDrafts) = resolution else {
            if case let .failure(failure) = resolution { return failure }
            fatalError("unreachable document resolution")
        }
        let post: AgentGroupChatPostResult
        do {
            post = try await store.postMessage(
                ownerUserID: context.ownerUserID,
                roomID: conversationID,
                draft: .init(
                    senderKind: .agent,
                    senderID: context.agentID,
                    content: content,
                    sourceRunID: context.runID,
                    causationID: context.deliveryID,
                    hopCount: context.hopCount + 1,
                    attachments: attachmentDrafts
                ),
                limits: limits
            )
        } catch {
            await references.releaseDocuments(references: documentReferences, callID: call.id)
            throw error
        }
        await references.consumeDocuments(references: documentReferences, callID: call.id)
        await roomChangeHandler(conversationID)
        let outcome = try Self.outcome(DirectSendResponse(
            conversationReference: conversationReference,
            messageReference: await references.messageReference(
                roomID: conversationID,
                messageID: post.message.id
            ),
            spawnedDeliveryCount: post.deliveries.count,
            routingStopReason: post.routingStopReason
        ))
        await recordSendOutcome(outcome, call: call)
        return outcome
    }

    private func sendTeam(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        if let replayed = await replayedSendOutcome(call) { return replayed }
        let arguments = try Self.arguments(call)
        let content = try Self.requiredString(arguments, key: "content")
        let lengthFailure = Self.messageLengthFailure(content)
        await recordMessageAttempt(content, rejected: lengthFailure != nil)
        if let lengthFailure { return lengthFailure }
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
        let resolution = try await resolveDocumentDrafts(arguments: arguments, callID: call.id)
        guard case let .ready(documentReferences, attachmentDrafts) = resolution else {
            if case let .failure(failure) = resolution { return failure }
            fatalError("unreachable document resolution")
        }
        let post: AgentGroupChatPostResult
        do {
            post = try await store.postMessage(
                ownerUserID: context.ownerUserID,
                roomID: teamRoomID,
                draft: .init(
                    senderKind: .agent,
                    senderID: context.agentID,
                    content: content,
                    mentionedAgentIDs: mentionAgentIDs,
                    sourceRunID: context.runID,
                    causationID: context.deliveryID,
                    hopCount: context.hopCount + 1,
                    attachments: attachmentDrafts
                ),
                limits: limits
            )
        } catch {
            await references.releaseDocuments(references: documentReferences, callID: call.id)
            throw error
        }
        await references.consumeDocuments(references: documentReferences, callID: call.id)
        await roomChangeHandler(teamRoomID)
        let outcome = try Self.outcome(
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
        await recordSendOutcome(outcome, call: call)
        return outcome
    }

    private func sendMessage(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        if let replayed = await replayedSendOutcome(call) { return replayed }
        let arguments = try Self.arguments(call)
        let content = try Self.requiredString(arguments, key: "content")
        let lengthFailure = Self.messageLengthFailure(content)
        await recordMessageAttempt(content, rejected: lengthFailure != nil)
        if let lengthFailure { return lengthFailure }
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
        let resolution = try await resolveDocumentDrafts(arguments: arguments, callID: call.id)
        guard case let .ready(documentReferences, attachmentDrafts) = resolution else {
            if case let .failure(failure) = resolution { return failure }
            fatalError("unreachable document resolution")
        }
        let post: AgentGroupChatPostResult
        do {
            post = try await store.postMessage(
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
                    hopCount: context.hopCount + 1,
                    attachments: attachmentDrafts
                ),
                limits: limits
            )
        } catch {
            await references.releaseDocuments(references: documentReferences, callID: call.id)
            throw error
        }
        await references.consumeDocuments(references: documentReferences, callID: call.id)
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
        let outcome = try Self.outcome(
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
        await recordSendOutcome(outcome, call: call)
        return outcome
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

    private func resolveDocumentDrafts(
        arguments: [String: Any],
        callID: String
    ) async throws -> DocumentDraftResolution {
        let documentReferences = try Self.optionalStringArray(arguments, key: "document_refs")
        let policy = AgentCommunicationPolicy.standard
        guard documentReferences.count <= policy.maximumDocumentsPerMessage else {
            await recordRejection(.tooManyDocumentRefs)
            return .failure(Self.structuredFailure(
                code: "too_many_document_refs",
                field: "document_refs",
                message: "每条消息最多可附加 \(policy.maximumDocumentsPerMessage) 个文档。",
                retryable: true,
                nextTool: Self.createDocumentToolName
            ))
        }
        guard Set(documentReferences).count == documentReferences.count else {
            await recordRejection(.duplicateDocumentRef)
            return .failure(Self.structuredFailure(
                code: "duplicate_document_ref",
                field: "document_refs",
                message: "document_refs 不能包含重复引用。",
                retryable: true
            ))
        }
        guard !documentReferences.isEmpty else {
            return .ready(references: [], drafts: [])
        }
        switch await references.reserveDocuments(
            references: documentReferences,
            callID: callID
        ) {
        case let .success(drafts):
            return .ready(references: documentReferences, drafts: drafts)
        case let .invalid(index):
            await recordRejection(.invalidDocumentRef)
            return .failure(Self.structuredFailure(
                code: "invalid_document_ref",
                field: "document_refs[\(index)]",
                message: "文档引用无效、已消费、属于其他 Run，或正在被另一条消息使用；请重新创建文档。",
                retryable: true,
                nextTool: Self.createDocumentToolName
            ))
        case let .integrityChanged(index):
            await recordRejection(.documentIntegrityChanged)
            return .failure(Self.structuredFailure(
                code: "document_integrity_changed",
                field: "document_refs[\(index)]",
                message: "本地文档草稿的大小或哈希已经变化，请重新创建文档。",
                retryable: true,
                nextTool: Self.createDocumentToolName
            ))
        }
    }

    private func replayedSendOutcome(_ call: AgentToolCall) async -> AgentToolOutcome? {
        let signature = "\(call.name)\n\(call.arguments)"
        switch await references.sendReceipt(callID: call.id, signature: signature) {
        case .missing:
            return nil
        case let .match(outcome):
            return outcome
        case .callIDConflict:
            return Self.structuredFailure(
                code: "tool_call_id_reused",
                field: nil,
                message: "同一工具调用 ID 不能用于不同的发送参数。",
                retryable: false
            )
        }
    }

    private func recordSendOutcome(_ outcome: AgentToolOutcome, call: AgentToolCall) async {
        await references.recordSendReceipt(
            callID: call.id,
            signature: "\(call.name)\n\(call.arguments)",
            outcome: outcome
        )
    }

    private func recordMessageAttempt(_ content: String, rejected: Bool) async {
        guard let localStore = store as? SQLiteAgentGroupChatStore else { return }
        try? await localStore.recordAgentMessageAttempt(
            ownerUserID: context.ownerUserID,
            characterCount: content.count,
            rejected: rejected,
            nowUnixMs: now()
        )
    }

    private func recordDocumentCreation(
        _ outcome: AgentDocumentCreationMetricOutcome,
        bytes: Int = 0
    ) async {
        guard let localStore = store as? SQLiteAgentGroupChatStore else { return }
        try? await localStore.recordAgentDocumentCreation(
            ownerUserID: context.ownerUserID,
            outcome: outcome,
            bytes: bytes,
            nowUnixMs: now()
        )
    }

    private func recordRejection(_ reason: AgentCommunicationRejectionMetricReason) async {
        guard let localStore = store as? SQLiteAgentGroupChatStore else { return }
        try? await localStore.recordAgentToolRejection(
            ownerUserID: context.ownerUserID,
            reason: reason,
            nowUnixMs: now()
        )
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
}
