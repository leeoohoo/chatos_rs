import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation

extension LocalAgentChatToolProvider {
    func openDirect(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func sendDirect(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

}
