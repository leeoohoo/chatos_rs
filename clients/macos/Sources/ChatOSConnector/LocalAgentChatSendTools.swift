import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation

extension LocalAgentChatToolProvider {
    func sendMessage(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        if let replayed = await replayedSendOutcome(call) { return replayed }
        let arguments = try Self.arguments(call)
        let content = try Self.requiredString(arguments, key: "content")
        let lengthFailure = Self.messageLengthFailure(content)
        await recordMessageAttempt(content, rejected: lengthFailure != nil)
        if let lengthFailure { return lengthFailure }
        let replyReference = try Self.optionalString(arguments, key: "reply_to_message_ref")
        let targetAgentReference = try Self.optionalString(arguments, key: "target_agent_ref")
        let teamReference = try Self.optionalString(arguments, key: "team_ref")
        let selectors = [replyReference, targetAgentReference, teamReference].compactMap { $0 }
        guard selectors.count <= 1 else {
            return Self.structuredFailure(
                code: "ambiguous_message_target",
                field: "message_target",
                message: "reply_to_message_ref、target_agent_ref 和 team_ref 最多只能提供一个。回复未读、主动私聊和主动发团队消息是同一发送工具的三种目标，不是三套运行入口。",
                retryable: true,
                nextTool: Self.sendMessageToolName
            )
        }

        guard let delivery = try await store.delivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), delivery.status == .running,
           delivery.roomID == context.roomID,
           delivery.targetAgentID == context.agentID,
           delivery.messageID == context.triggerMessageID,
           delivery.rootMessageID == context.rootMessageID else {
            return Self.structuredFailure(
                code: "agent_cycle_not_running",
                field: nil,
                message: "这次 Agent 通讯周期已经结束或被替换，不能继续发送。请等待下一次唤醒或由客户端重试该周期。",
                retryable: false
            )
        }

        let targetRoomID: String
        let replyMessage: ProjectAgentMessage?
        if let replyReference {
            guard let authority = await references.messageAuthority(reference: replyReference),
                  let message = try await store.message(
                    ownerUserID: context.ownerUserID,
                    roomID: authority.roomID,
                    messageID: authority.messageID
                  ) else {
                return Self.structuredFailure(
                    code: "invalid_message_ref",
                    field: "reply_to_message_ref",
                    message: "回复消息引用无效或已经过期，请重新读取全部未读或任务来源。",
                    retryable: true,
                    nextTool: Self.readAllUnreadToolName
                )
            }
            targetRoomID = authority.roomID
            replyMessage = message
        } else if let targetAgentReference {
            guard let targetAgentID = await references.agentID(reference: targetAgentReference),
                  targetAgentID != context.agentID else {
                return Self.structuredFailure(
                    code: "invalid_agent_ref",
                    field: "target_agent_ref",
                    message: "目标 Agent 引用无效、已停用或指向当前 Agent，请重新读取账户 Agent 列表。",
                    retryable: true,
                    nextTool: Self.workspaceSnapshotToolName
                )
            }
            do {
                let direct = try await store.openAgentDirect(
                    ownerUserID: context.ownerUserID,
                    initiatingAgentID: context.agentID,
                    targetAgentID: targetAgentID
                )
                targetRoomID = direct.id
                replyMessage = nil
            } catch AgentGroupChatError.notFound {
                return Self.structuredFailure(
                    code: "target_agent_unavailable",
                    field: "target_agent_ref",
                    message: "目标 Agent 已不存在或停用，请重新读取账户 Agent 列表。",
                    retryable: true,
                    nextTool: Self.workspaceSnapshotToolName
                )
            }
        } else if let teamReference {
            guard let teamRoomID = await references.teamID(reference: teamReference),
                  let team = try await store.room(
                    ownerUserID: context.ownerUserID,
                    roomID: teamRoomID
                  ), team.status == .active,
                  team.conversationKind == .projectTeam else {
                return Self.structuredFailure(
                    code: "invalid_team_ref",
                    field: "team_ref",
                    message: "团队引用无效或已经过期，请重新读取账户团队列表。",
                    retryable: true,
                    nextTool: Self.workspaceSnapshotToolName
                )
            }
            targetRoomID = teamRoomID
            replyMessage = nil
        } else {
            guard let trigger = try await store.message(
                ownerUserID: context.ownerUserID,
                roomID: context.roomID,
                messageID: context.triggerMessageID
            ) else {
                return Self.structuredFailure(
                    code: "trigger_message_unavailable",
                    field: nil,
                    message: "唤醒消息已经不可用，请重新读取全部未读后再回复。",
                    retryable: true,
                    nextTool: Self.readAllUnreadToolName
                )
            }
            targetRoomID = context.roomID
            replyMessage = trigger
        }

        let members = try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: targetRoomID
        )
        let activeMemberIDs = Set(members.filter { $0.status == .active }.map(\.agentID))
        guard activeMemberIDs.contains(context.agentID) else {
            return Self.structuredFailure(
                code: "target_conversation_not_accessible",
                field: nil,
                message: "当前 Agent 无权向这条消息所属会话发送。请继续处理其他未读和任务，不要把路由失败升级为整轮阻塞。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        let mentionReferences = try Self.optionalStringArray(arguments, key: "mention_agent_refs")
        var mentionAgentIDs: [String] = []
        for (index, reference) in mentionReferences.enumerated() {
            guard let agentID = await references.agentID(reference: reference) else {
                return Self.structuredFailure(
                    code: "invalid_agent_ref",
                    field: "mention_agent_refs[\(index)]",
                    message: "被 @ 的 Agent 引用无效或已经过期，请重新读取账户 Agent 与团队列表。",
                    retryable: true,
                    nextTool: Self.workspaceSnapshotToolName
                )
            }
            guard activeMemberIDs.contains(agentID) else {
                return Self.structuredFailure(
                    code: "mention_not_in_target_conversation",
                    field: "mention_agent_refs[\(index)]",
                    message: "被 @ 的 Agent 不属于目标会话；请去掉该 @，或直接用 target_agent_ref 主动私聊并唤醒目标 Agent。",
                    retryable: true,
                    nextTool: Self.sendMessageToolName
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
                roomID: targetRoomID,
                draft: .init(
                    senderKind: .agent,
                    senderID: context.agentID,
                    content: content,
                    mentionedAgentIDs: mentionAgentIDs,
                    replyToMessageID: replyMessage?.id,
                    sourceRunID: context.runID,
                    causationID: context.deliveryID,
                    rootMessageID: replyMessage?.rootMessageID,
                    hopCount: replyMessage.map { min(64, $0.hopCount + 1) }
                        ?? min(64, context.hopCount + 1),
                    attachments: attachmentDrafts
                ),
                limits: limits
            )
        } catch AgentGroupChatError.notMember {
            await references.releaseDocuments(references: documentReferences, callID: call.id)
            return Self.structuredFailure(
                code: "target_conversation_not_accessible",
                field: nil,
                message: "发送期间目标会话的成员关系发生变化。本轮 Agent 仍可继续处理其他未读和任务；不要把这次发送失败升级成整轮阻塞。",
                retryable: true,
                nextTool: Self.readAllUnreadToolName
            )
        } catch {
            await references.releaseDocuments(references: documentReferences, callID: call.id)
            throw error
        }
        await references.consumeDocuments(references: documentReferences, callID: call.id)
        await recordSuccessfulMessage(content, documentCount: attachmentDrafts.count)
        await roomChangeHandler(targetRoomID)
        if let replyMessage {
            _ = try? await store.markMessagesRead(
                ownerUserID: context.ownerUserID,
                roomID: targetRoomID,
                agentID: context.agentID,
                throughMessageID: replyMessage.id,
                nowUnixMs: now()
            )
        }
        let outcome = try Self.outcome(
            SendResponse(
                messageReference: await references.messageReference(
                    roomID: targetRoomID,
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
