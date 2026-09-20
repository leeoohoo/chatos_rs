import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    func bootstrap(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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
    func workspaceSnapshot(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func getTrigger(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func listMembers(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func readUnread(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func unreadResponse(
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

    func messageResponse(
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

    func readAllUnread(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func sendInboxMessage(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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
        await recordSuccessfulMessage(content, documentCount: attachmentDrafts.count)
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
}
