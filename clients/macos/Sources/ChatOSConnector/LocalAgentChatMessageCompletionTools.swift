import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation

extension LocalAgentChatToolProvider {
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
