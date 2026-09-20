import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation

extension LocalAgentChatToolProvider {
    func readMessages(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func readAttachment(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func createDocument(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func markRead(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func completeHeartbeat(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func completeManagerCycle(_ call: AgentToolCall) async throws -> AgentToolOutcome {
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

    func resolveDocumentDrafts(
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

    func replayedSendOutcome(_ call: AgentToolCall) async -> AgentToolOutcome? {
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

    func recordSendOutcome(_ outcome: AgentToolOutcome, call: AgentToolCall) async {
        await references.recordSendReceipt(
            callID: call.id,
            signature: "\(call.name)\n\(call.arguments)",
            outcome: outcome
        )
    }

    func recordMessageAttempt(_ content: String, rejected: Bool) async {
        guard let localStore = store as? SQLiteAgentGroupChatStore else { return }
        try? await localStore.recordAgentMessageAttempt(
            ownerUserID: context.ownerUserID,
            characterCount: content.count,
            rejected: rejected,
            nowUnixMs: now()
        )
    }

    func recordDocumentCreation(
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

    func recordRejection(_ reason: AgentCommunicationRejectionMetricReason) async {
        guard let localStore = store as? SQLiteAgentGroupChatStore else { return }
        try? await localStore.recordAgentToolRejection(
            ownerUserID: context.ownerUserID,
            reason: reason,
            nowUnixMs: now()
        )
    }
}
