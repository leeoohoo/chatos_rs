import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentGroupChatScheduler {
    /// Stops both queued and active work for the current project. The caller should first cancel
    /// its in-process scheduler task so no model or Plugin call remains active while SQLite closes
    /// the durable queue.
    @discardableResult
    public func stopProject(ownerUserID: String, projectID: String) async throws -> Int {
        let store = try await service.store()
        guard let room = try await store.activeRoom(
            ownerUserID: ownerUserID,
            projectID: projectID
        ) else { throw AgentGroupChatError.notFound }
        let stopped = try await store.stopOutstandingDeliveries(
            ownerUserID: ownerUserID,
            roomID: room.id,
            reason: "用户已停止当前项目中的全部本地 Agent。",
            nowUnixMs: now()
        )
        await service.publishChange(.init(
            ownerUserID: ownerUserID,
            roomID: room.id,
            kind: .roomUpdated
        ))
        return stopped
    }

    /// Explicitly resumes one durable running delivery. This is intentionally not automatic:
    /// a checkpoint may contain an in-flight side-effecting Plugin call, and AgentRuntime must
    /// surface that as `needsReview` instead of replaying it after a crash.
    public func resumeDelivery(
        ownerUserID: String,
        projectID: String,
        deliveryID: String
    ) async throws -> DeliveryAttemptReceipt {
        let store = try await service.store()
        guard let delivery = try await store.delivery(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
        ), let room = try await store.room(
            ownerUserID: ownerUserID,
            roomID: delivery.roomID
        ), room.projectID == projectID, room.status == .active,
           delivery.status == .running,
           let member = try await store.listMembers(
            ownerUserID: ownerUserID,
            roomID: room.id
           ).first(where: { $0.agentID == delivery.targetAgentID }),
           let savedRun = try await store.run(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
           ) else {
            throw AgentGroupChatError.conflict
        }
        do {
            return try await runClaimedDelivery(
                store: store,
                ownerUserID: ownerUserID,
                projectID: projectID,
                room: room,
                member: member,
                delivery: delivery,
                savedRun: savedRun
            )
        } catch {
            var paused = savedRun
            paused.checkpoint.status = .paused
            paused.checkpoint.stopReason = "恢复失败：\(Self.failureDetail(error))"
            paused.events.append(.init(
                kind: "resume_failed",
                detail: paused.checkpoint.stopReason ?? "恢复失败",
                modelCalls: paused.checkpoint.modelCalls
            ))
            paused.updatedAtUnixMs = max(now(), paused.updatedAtUnixMs)
            try await store.saveRun(paused)
            await service.publishChange(.init(
                ownerUserID: ownerUserID,
                roomID: paused.context.roomID,
                agentID: paused.context.agentID,
                runID: paused.id,
                kind: .runUpdated
            ))
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .suspended,
                detail: paused.checkpoint.stopReason
            )
        }
    }

    /// Explicit Human authorization to retry the single write/billable call whose outcome could
    /// not be durably recorded. Normal resume deliberately refuses to replay such a call; this
    /// entry point clears only the in-flight marker while preserving the pending call and its
    /// stable call id, so idempotent local tools can reconcile an already-applied result.
    public func retryInterruptedDelivery(
        ownerUserID: String,
        projectID: String,
        deliveryID: String
    ) async throws -> DeliveryAttemptReceipt {
        let store = try await service.store()
        guard let delivery = try await store.delivery(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
        ), let room = try await store.room(
            ownerUserID: ownerUserID,
            roomID: delivery.roomID
        ), room.projectID == projectID, room.status == .active,
           delivery.status == .running,
           var savedRun = try await store.run(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
           ), savedRun.checkpoint.status == .needsReview,
           let inFlightCallID = savedRun.checkpoint.inFlightCallID,
           savedRun.checkpoint.pendingCalls.contains(where: { $0.id == inFlightCallID }) else {
            throw AgentGroupChatError.conflict
        }

        savedRun.checkpoint.inFlightCallID = nil
        savedRun.checkpoint.status = .paused
        savedRun.checkpoint.stopReason = nil
        // The interrupted attempt may have consumed the complete run budget while waiting for
        // its side effect. An explicit Human retry is a new attempt and needs a fresh deadline;
        // model/tool history remains intact for reconciliation.
        savedRun.checkpoint.elapsedSeconds = 0
        savedRun.events.append(.init(
            kind: "retry_authorized",
            detail: "Human 已明确重试中断步骤：\(inFlightCallID)",
            modelCalls: savedRun.checkpoint.modelCalls
        ))
        if delivery.triggerKind == .todo,
           let todo = try await store.todoForDelivery(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
           ), todo.status == .blocked {
            _ = try await store.updateAgentTodoAfterHumanReview(
                ownerUserID: ownerUserID,
                agentID: todo.agentID,
                todoID: todo.id,
                update: .init(status: .pending, blockedReason: ""),
                nowUnixMs: now()
            )
            _ = try await store.updateAgentTodo(
                ownerUserID: ownerUserID,
                agentID: todo.agentID,
                todoID: todo.id,
                update: .init(status: .inProgress, blockedReason: ""),
                nowUnixMs: now()
            )
            _ = try await store.appendAgentTodoProgress(
                ownerUserID: ownerUserID,
                agentID: todo.agentID,
                todoID: todo.id,
                kind: .progress,
                runID: savedRun.context.runID,
                stage: "retry_authorized",
                detail: "Human 已确认重试中断步骤，Todo 已恢复执行。",
                assetUpdateSuggestions: [],
                nowUnixMs: now()
            )
        }
        savedRun.updatedAtUnixMs = max(now(), savedRun.updatedAtUnixMs)
        try await store.saveRun(savedRun)
        await service.publishChange(.init(
            ownerUserID: ownerUserID,
            roomID: savedRun.context.roomID,
            agentID: savedRun.context.agentID,
            runID: savedRun.id,
            kind: .runUpdated
        ))
        return try await resumeDelivery(
            ownerUserID: ownerUserID,
            projectID: projectID,
            deliveryID: deliveryID
        )
    }

    public func abandonDelivery(
        ownerUserID: String,
        projectID: String,
        deliveryID: String
    ) async throws {
        let store = try await service.store()
        guard let delivery = try await store.delivery(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
        ), let room = try await store.room(
            ownerUserID: ownerUserID,
            roomID: delivery.roomID
        ), room.projectID == projectID, room.status == .active,
           delivery.status == .running,
           var run = try await store.run(ownerUserID: ownerUserID, deliveryID: deliveryID) else {
            throw AgentGroupChatError.conflict
        }
        let detail = "用户已结束这个未完成的本地 Agent Run。"
        run.checkpoint.status = .failed
        run.checkpoint.stopReason = detail
        run.events.append(.init(
            kind: "abandoned",
            detail: detail,
            modelCalls: run.checkpoint.modelCalls
        ))
        run.updatedAtUnixMs = max(now(), run.updatedAtUnixMs)
        try await store.saveRun(run)
        if delivery.triggerKind == .todo {
            try await failDeliveryAndNotifyManager(
                store: store,
                ownerUserID: ownerUserID,
                delivery: delivery,
                runID: run.context.runID,
                detail: detail,
                stage: "abandoned"
            )
        } else {
            _ = try await store.failDelivery(
                ownerUserID: ownerUserID,
                deliveryID: deliveryID,
                error: detail,
                nowUnixMs: now()
            )
        }
        await service.publishChange(.init(
            ownerUserID: ownerUserID,
            roomID: run.context.roomID,
            agentID: run.context.agentID,
            runID: run.id,
            kind: .runUpdated
        ))
    }

    func runClaimedDeliveryHandlingFailure(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        projectID: String,
        room: ProjectAgentRoom,
        member: ProjectAgentRoomMember,
        delivery: ProjectAgentDelivery
    ) async throws -> DeliveryAttemptReceipt {
        do {
            // A failed Todo delivery can be explicitly reactivated by moving its Todo back to
            // pending. Delivery and Run are intentionally one-to-one, so the next claim must
            // resume the durable checkpoint instead of trying to create a second Run.
            let savedRun = try await store.run(
                ownerUserID: ownerUserID,
                deliveryID: delivery.id
            )
            return try await runClaimedDelivery(
                store: store,
                ownerUserID: ownerUserID,
                projectID: projectID,
                room: room,
                member: member,
                delivery: delivery,
                savedRun: savedRun
            )
        } catch {
            let detail = Self.failureDetail(error)
            var savedRunID: String?
            if var savedRun = try await store.run(
                ownerUserID: ownerUserID,
                deliveryID: delivery.id
            ) {
                savedRunID = savedRun.context.runID
                savedRun.checkpoint.status = .failed
                savedRun.checkpoint.stopReason = detail
                savedRun.events.append(.init(
                    kind: "stopped",
                    detail: detail,
                    modelCalls: savedRun.checkpoint.modelCalls
                ))
                savedRun.updatedAtUnixMs = max(now(), savedRun.updatedAtUnixMs)
                try await store.saveRun(savedRun)
                await service.publishChange(.init(
                    ownerUserID: ownerUserID,
                    roomID: savedRun.context.roomID,
                    agentID: savedRun.context.agentID,
                    runID: savedRun.id,
                    kind: .runUpdated
                ))
            }
            if try await store.delivery(ownerUserID: ownerUserID, deliveryID: delivery.id)?.status == .running {
                try await failDeliveryAndNotifyManager(
                    store: store,
                    ownerUserID: ownerUserID,
                    delivery: delivery,
                    runID: savedRunID,
                    detail: detail,
                    stage: "executor_failed"
                )
            }
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .failed,
                detail: detail
            )
        }
    }
}
