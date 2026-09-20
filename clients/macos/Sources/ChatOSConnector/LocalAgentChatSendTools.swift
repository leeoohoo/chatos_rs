import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation

extension LocalAgentChatToolProvider {
    func sendTeam(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func sendMessage(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

}
