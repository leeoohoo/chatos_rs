import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentGroupChatScheduler {
    /// Drains all currently reachable deliveries in one project. Each round first claims at most
    /// one delivery per Agent, then runs different Agents concurrently. Waiting for the round to
    /// finish before claiming again preserves per-Agent serialization while re-reading the member
    /// queue lets an Agent's `@mention` wake another Agent without server polling.
    public func drainProject(
        ownerUserID: String,
        projectID: String,
        maximumRuns: Int = 32
    ) async throws -> [DeliveryAttemptReceipt] {
        guard maximumRuns > 0 else { return [] }
        let store = try await service.store()
        guard let room = try await store.activeRoom(
            ownerUserID: ownerUserID,
            projectID: projectID
        ) else { throw AgentGroupChatError.notFound }

        return try await drain(
            store: store,
            ownerUserID: ownerUserID,
            room: room,
            maximumRuns: maximumRuns
        )
    }

    /// Drains one durable conversation, whether it is a project team or a private chat.
    public func drainConversation(
        ownerUserID: String,
        roomID: String,
        maximumRuns: Int = 32
    ) async throws -> [DeliveryAttemptReceipt] {
        guard maximumRuns > 0 else { return [] }
        let store = try await service.store()
        guard let room = try await store.room(ownerUserID: ownerUserID, roomID: roomID),
              room.status == .active else {
            throw AgentGroupChatError.notFound
        }
        guard try await store.hasPendingDeliveries(
            ownerUserID: ownerUserID,
            roomID: roomID,
            lane: .manager
        ) else { return [] }
        return try await drain(
            store: store,
            ownerUserID: ownerUserID,
            room: room,
            maximumRuns: maximumRuns
        )
    }

    /// Drains only the communication/manager lane for one conversation. This intentionally does
    /// not acquire the account-wide executor recovery lease: durable lane constraints prevent a
    /// duplicate claim, while new Human messages remain responsive during long project Todos.
    public func drainCommunication(
        ownerUserID: String,
        roomID: String,
        maximumRuns: Int = 32
    ) async throws -> [DeliveryAttemptReceipt] {
        guard maximumRuns > 0 else { return [] }
        let store = try await service.store()
        guard let room = try await store.room(ownerUserID: ownerUserID, roomID: roomID),
              room.status == .active else {
            throw AgentGroupChatError.notFound
        }
        return try await drain(
            store: store,
            ownerUserID: ownerUserID,
            room: room,
            maximumRuns: maximumRuns,
            lane: .manager
        )
    }

    /// Drains manager lanes across every active team and private conversation without waiting for
    /// executor recovery. The background heartbeat uses this path so communication does not depend
    /// on which conversation, if any, is currently visible in the UI.
    public func drainCommunications(
        ownerUserID: String,
        maximumRuns: Int = 64
    ) async throws -> [DeliveryAttemptReceipt] {
        guard maximumRuns > 0 else { return [] }
        let store = try await service.store()
        _ = try await store.quarantineNeedsReviewManagerDeliveries(
            ownerUserID: ownerUserID,
            nowUnixMs: now()
        )
        guard try await store.hasPendingDeliveries(
            ownerUserID: ownerUserID,
            lane: .manager
        ) else { return [] }
        return try await drainAccountQueue(
            store: store,
            ownerUserID: ownerUserID,
            maximumRuns: maximumRuns,
            lane: .manager
        )
    }

    /// Drains all account-owned conversations so a Relay message that opens another private
    /// conversation is delivered without requiring that destination to be visible in the UI.
    public func drainAccount(
        ownerUserID: String,
        maximumRuns: Int = 64
    ) async throws -> [DeliveryAttemptReceipt] {
        guard maximumRuns > 0 else { return [] }
        try Task.checkCancellation()
        return try await drainAccountPass(
            ownerUserID: ownerUserID,
            maximumRuns: maximumRuns
        )
    }

    func drainAccountPass(
        ownerUserID: String,
        maximumRuns: Int
    ) async throws -> [DeliveryAttemptReceipt] {
        let store = try await service.store()
        guard try await store.hasOutstandingDeliveries(ownerUserID: ownerUserID) else {
            return []
        }
        let hasInterruptedRuns = try await store.listInterruptedRuns(
            ownerUserID: ownerUserID,
            limit: 1
        ).isEmpty == false
        let hasFreshDeliveries = try await store.hasPendingDeliveries(ownerUserID: ownerUserID)
        if maximumRuns > 1, hasInterruptedRuns, hasFreshDeliveries {
            let recoveryBudget = max(1, maximumRuns / 2)
            let freshBudget = maximumRuns - recoveryBudget
            var recovered: [DeliveryAttemptReceipt] = []
            var fresh: [DeliveryAttemptReceipt] = []
            var firstError: Error?
            await withTaskGroup(of: AccountPassCompletion.self) { group in
                group.addTask {
                    do {
                        return .recovered(try await recoverInterruptedRuns(
                            store: store,
                            ownerUserID: ownerUserID,
                            maximumRuns: recoveryBudget
                        ))
                    } catch {
                        return .failed(error)
                    }
                }
                group.addTask {
                    do {
                        return .fresh(try await drainAccountQueue(
                            store: store,
                            ownerUserID: ownerUserID,
                            maximumRuns: freshBudget,
                            lane: nil
                        ))
                    } catch {
                        return .failed(error)
                    }
                }
                for await completion in group {
                    switch completion {
                    case let .recovered(receipts):
                        recovered = receipts
                    case let .fresh(receipts):
                        fresh = receipts
                    case let .failed(error):
                        if firstError == nil { firstError = error }
                    }
                }
            }
            if let firstError { throw firstError }

            var results = recovered + fresh
            if results.count < maximumRuns {
                results.append(contentsOf: try await recoverInterruptedRuns(
                    store: store,
                    ownerUserID: ownerUserID,
                    maximumRuns: maximumRuns - results.count
                ))
            }
            if results.count < maximumRuns {
                results.append(contentsOf: try await drainAccountQueue(
                    store: store,
                    ownerUserID: ownerUserID,
                    maximumRuns: maximumRuns - results.count,
                    lane: nil
                ))
            }
            return results
        }
        var results = try await recoverInterruptedRuns(
            store: store,
            ownerUserID: ownerUserID,
            maximumRuns: maximumRuns
        )
        if results.count < maximumRuns {
            results.append(contentsOf: try await drainAccountQueue(
                store: store,
                ownerUserID: ownerUserID,
                maximumRuns: maximumRuns - results.count,
                lane: nil
            ))
        }
        return results
    }

    private enum AccountPassCompletion: @unchecked Sendable {
        case recovered([DeliveryAttemptReceipt])
        case fresh([DeliveryAttemptReceipt])
        case failed(Error)
    }

    func drainAccountQueue(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        maximumRuns: Int,
        lane: LocalAgentRunLane?
    ) async throws -> [DeliveryAttemptReceipt] {
        guard maximumRuns > 0,
              try await store.hasPendingDeliveries(
                  ownerUserID: ownerUserID,
                  lane: lane
              ) else {
            return []
        }
        let executionPriority: LocalAgentExecutionPriority = lane == .manager
            ? .interactive
            : .background
        var claimContext = try await accountDrainClaimContext(
            store: store,
            ownerUserID: ownerUserID
        )
        return try await withThrowingTaskGroup(
            of: ScheduledWorkCompletion.self,
            returning: [DeliveryAttemptReceipt].self
        ) { group in
            var completed: [OrderedDeliveryAttemptReceipt] = []
            var inFlightCount = 0
            var nextOrder = 0
            var nextAgentIndex = 0
            var shouldSchedule = true
            var firstError: Error?

            while inFlightCount > 0
                    || (shouldSchedule
                        && !Task.isCancelled
                        && completed.count < maximumRuns) {
                var exhaustedAgentIDs = Set<String>()
                while shouldSchedule,
                      !Task.isCancelled,
                      !claimContext.agentIDs.isEmpty,
                      exhaustedAgentIDs.count < claimContext.agentIDs.count,
                      completed.count + inFlightCount < maximumRuns {
                    let agentID = claimContext.agentIDs[
                        nextAgentIndex % claimContext.agentIDs.count
                    ]
                    nextAgentIndex = (nextAgentIndex + 1) % claimContext.agentIDs.count
                    guard !exhaustedAgentIDs.contains(agentID) else { continue }
                    do {
                        try await runtime.checkExecutionAdmission()
                    } catch {
                        if firstError == nil { firstError = error }
                        shouldSchedule = false
                        break
                    }
                    let executionPermit: LocalAgentExecutionPermit?
                    if inFlightCount == 0 {
                        executionPermit = await executionGovernor.acquire(
                            priority: executionPriority
                        )
                    } else {
                        executionPermit = await executionGovernor.tryAcquire()
                    }
                    guard let executionPermit else { break }
                    if Task.isCancelled {
                        await executionPermit.releaseAndWait()
                        shouldSchedule = false
                        break
                    }
                    let delivery: ProjectAgentDelivery?
                    do {
                        if let lane {
                            delivery = try await store.claimNextDelivery(
                                ownerUserID: ownerUserID,
                                agentID: agentID,
                                lane: lane,
                                nowUnixMs: now()
                            )
                        } else {
                            delivery = try await store.claimNextDelivery(
                                ownerUserID: ownerUserID,
                                agentID: agentID,
                                nowUnixMs: now()
                            )
                        }
                    } catch {
                        await executionPermit.releaseAndWait()
                        if firstError == nil { firstError = error }
                        shouldSchedule = false
                        break
                    }
                    guard let delivery else {
                        await executionPermit.releaseAndWait()
                        exhaustedAgentIDs.insert(agentID)
                        continue
                    }
                    do {
                        let room: ProjectAgentRoom
                        if let known = claimContext.roomByID[delivery.roomID] {
                            room = known
                        } else if let loaded = try await store.room(
                            ownerUserID: ownerUserID,
                            roomID: delivery.roomID
                        ) {
                            room = loaded
                            claimContext.roomByID[loaded.id] = loaded
                        } else {
                            throw AgentGroupChatError.notFound
                        }
                        let key = Self.memberKey(roomID: room.id, agentID: agentID)
                        let member: ProjectAgentRoomMember
                        if let known = claimContext.memberByRoomAndAgent[key] {
                            member = known
                        } else if let loaded = try await store.listMembers(
                            ownerUserID: ownerUserID,
                            roomID: room.id
                        ).first(where: { $0.agentID == agentID && $0.status == .active }) {
                            member = loaded
                            claimContext.memberByRoomAndAgent[key] = loaded
                        } else {
                            throw AgentGroupChatError.notMember
                        }
                        let work = ClaimedWork(
                            order: nextOrder,
                            room: room,
                            member: member,
                            delivery: delivery,
                            executionPermit: executionPermit
                        )
                        nextOrder += 1
                        guard let prepared = try await prepareClaimedWork(
                            work,
                            store: store,
                            ownerUserID: ownerUserID
                        ) else {
                            continue
                        }
                        inFlightCount += 1
                        group.addTask {
                            .finished(await runPreparedClaimedWork(
                                prepared,
                                store: store,
                                ownerUserID: ownerUserID
                            ))
                        }
                    } catch {
                        await executionPermit.releaseAndWait()
                        if firstError == nil { firstError = error }
                        shouldSchedule = false
                        break
                    }
                }

                guard inFlightCount > 0 else { break }
                guard let completion = try await group.next() else { break }
                inFlightCount -= 1
                switch completion {
                case let .finished(receipt):
                    if let receipt { completed.append(receipt) }
                case let .failed(error):
                    if firstError == nil { firstError = error }
                    shouldSchedule = false
                }
                if shouldSchedule, !Task.isCancelled {
                    do {
                        guard try await store.hasPendingDeliveries(
                            ownerUserID: ownerUserID,
                            lane: lane
                        ) else {
                            shouldSchedule = false
                            continue
                        }
                        claimContext = try await accountDrainClaimContext(
                            store: store,
                            ownerUserID: ownerUserID
                        )
                        if !claimContext.agentIDs.isEmpty {
                            nextAgentIndex %= claimContext.agentIDs.count
                        } else {
                            nextAgentIndex = 0
                        }
                    } catch {
                        if firstError == nil { firstError = error }
                        shouldSchedule = false
                    }
                }
            }

            if let firstError { throw firstError }
            completed.sort { $0.order < $1.order }
            return completed.map(\.receipt)
        }
    }

    private struct AccountDrainClaimContext {
        var roomByID: [String: ProjectAgentRoom]
        var memberByRoomAndAgent: [String: ProjectAgentRoomMember]
        var agentIDs: [String]
    }

    private func accountDrainClaimContext(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String
    ) async throws -> AccountDrainClaimContext {
        let teams = try await store.listRooms(ownerUserID: ownerUserID, includeArchived: false)
        let directs = try await store.listDirectConversations(
            ownerUserID: ownerUserID,
            includeArchived: false
        )
        let roomByID = Dictionary(uniqueKeysWithValues: (teams + directs).map { ($0.id, $0) })
        var memberByRoomAndAgent: [String: ProjectAgentRoomMember] = [:]
        var agentIDs: [String] = []
        var seenAgentIDs = Set<String>()
        for member in try await store.listActiveMembers(ownerUserID: ownerUserID) {
            guard roomByID[member.roomID] != nil else { continue }
            memberByRoomAndAgent[Self.memberKey(
                roomID: member.roomID,
                agentID: member.agentID
            )] = member
            if seenAgentIDs.insert(member.agentID).inserted {
                agentIDs.append(member.agentID)
            }
        }
        return .init(
            roomByID: roomByID,
            memberByRoomAndAgent: memberByRoomAndAgent,
            agentIDs: agentIDs
        )
    }

    /// Chat surfaces only append/display messages. Recovery belongs to the Agent trigger runtime.
    /// Atomic durable claims and the active-delivery registry exclude live work across concurrent
    /// drain callers without holding an account-wide lease for the lifetime of a model Run. Any remaining
    /// durable `running` checkpoint was left behind by an app exit or interrupted provider request.
    /// Resuming is safe even with an in-flight write marker because AgentRuntime converts that
    /// checkpoint to `needsReview` before any replay. Review states are never replayed
    /// automatically, but are reconciled onto the shared Todo board as a visible blocker.
    func recoverInterruptedRuns(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        maximumRuns: Int
    ) async throws -> [DeliveryAttemptReceipt] {
        let runs = try await store.listInterruptedRuns(ownerUserID: ownerUserID, limit: 500)
        return try await withThrowingTaskGroup(
            of: ScheduledWorkCompletion.self,
            returning: [DeliveryAttemptReceipt].self
        ) { group in
            var completed: [OrderedDeliveryAttemptReceipt] = []
            var nextRunIndex = 0
            var inFlightCount = 0
            var shouldSchedule = true
            var firstError: Error?

            while inFlightCount > 0
                    || (shouldSchedule
                        && !Task.isCancelled
                        && nextRunIndex < runs.count
                        && completed.count < maximumRuns) {
                while shouldSchedule,
                      !Task.isCancelled,
                      nextRunIndex < runs.count,
                      completed.count + inFlightCount < maximumRuns {
                    do {
                        try await runtime.checkExecutionAdmission()
                    } catch {
                        if firstError == nil { firstError = error }
                        shouldSchedule = false
                        break
                    }
                    let executionPermit: LocalAgentExecutionPermit?
                    if inFlightCount == 0 {
                        executionPermit = await executionGovernor.acquire(priority: .background)
                    } else {
                        executionPermit = await executionGovernor.tryAcquire()
                    }
                    guard let executionPermit else { break }
                    if Task.isCancelled {
                        await executionPermit.releaseAndWait()
                        shouldSchedule = false
                        break
                    }
                    let order = nextRunIndex
                    let run = runs[nextRunIndex]
                    nextRunIndex += 1
                    let deliveryID = run.context.deliveryID
                    guard await activeDeliveryRegistry.acquire(deliveryID: deliveryID) else {
                        await executionPermit.releaseAndWait()
                        continue
                    }
                    inFlightCount += 1
                    group.addTask {
                        do {
                            let receipt = try await recoverInterruptedRun(
                                store: store,
                                ownerUserID: ownerUserID,
                                run: run
                            )
                            await activeDeliveryRegistry.unregister(deliveryID: deliveryID)
                            await executionPermit.releaseAndWait()
                            return .finished(receipt.map {
                                OrderedDeliveryAttemptReceipt(order: order, receipt: $0)
                            })
                        } catch {
                            await activeDeliveryRegistry.unregister(deliveryID: deliveryID)
                            await executionPermit.releaseAndWait()
                            return .failed(error)
                        }
                    }
                }

                guard inFlightCount > 0 else { break }
                guard let completion = try await group.next() else { break }
                inFlightCount -= 1
                switch completion {
                case let .finished(receipt):
                    if let receipt {
                        completed.append(receipt)
                    }
                case let .failed(error):
                    if firstError == nil { firstError = error }
                    shouldSchedule = false
                }
            }

            if let firstError { throw firstError }
            completed.sort { $0.order < $1.order }
            return completed.map(\.receipt)
        }
    }

    private enum ScheduledWorkCompletion: @unchecked Sendable {
        case finished(OrderedDeliveryAttemptReceipt?)
        case failed(Error)
    }

    private func recoverInterruptedRun(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        run: LocalAgentGroupChatRun
    ) async throws -> DeliveryAttemptReceipt? {
        let deliveryID = run.context.deliveryID
        // Re-read after acquiring ownership. A Human retry may have changed both the
        // checkpoint and Todo between the account snapshot and this recovery attempt.
        guard let currentDelivery = try await store.delivery(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
        ), currentDelivery.status == .running,
           let currentRun = try await store.run(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
           ) else {
            return nil
        }
        if Self.interruptedCheckpointDisposition(currentRun.checkpoint) == .needsReview {
            let detail = currentRun.checkpoint.stopReason
                ?? "工具调用已开始但结果未被持久确认，需要检查副作用后再决定是否重试。"
            if currentRun.checkpoint.status != .needsReview {
                var reviewedRun = currentRun
                reviewedRun.checkpoint.status = .needsReview
                reviewedRun.checkpoint.stopReason = detail
                reviewedRun.events.append(.init(
                    kind: "needs_review",
                    detail: detail,
                    modelCalls: reviewedRun.checkpoint.modelCalls
                ))
                reviewedRun.updatedAtUnixMs = max(now(), reviewedRun.updatedAtUnixMs)
                try await store.saveRun(reviewedRun)
                await service.publishChange(.init(
                    ownerUserID: ownerUserID,
                    roomID: reviewedRun.context.roomID,
                    agentID: reviewedRun.context.agentID,
                    runID: reviewedRun.id,
                    kind: .runUpdated
                ))
            }
            if currentDelivery.lane == .executor {
                try await suspendTodoForReview(
                    store: store,
                    ownerUserID: ownerUserID,
                    delivery: currentDelivery,
                    runID: currentRun.context.runID,
                    detail: detail
                )
            } else {
                _ = try await store.failDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: currentDelivery.id,
                    error: detail,
                    nowUnixMs: now()
                )
            }
            return .init(
                deliveryID: currentDelivery.id,
                agentID: currentDelivery.targetAgentID,
                outcome: .suspended,
                detail: detail
            )
        }
        var isRecoveryEligible = Self.isAutomaticTriggerRecoveryEligible(
            currentRun.checkpoint
        )
        // Older builds could leave a Todo blocked while its durable delivery stayed
        // running and its Run later paused on repeated identity rejections. Once the
        // Human explicitly resolves that blocker, the Todo becomes pending but cannot
        // be claimed again because the original delivery is still running. Repair that
        // split state in place and resume the same durable Run.
        if !isRecoveryEligible,
           currentDelivery.triggerKind == .todo,
           currentRun.checkpoint.status == .paused,
           currentRun.checkpoint.pendingCalls.isEmpty,
           currentRun.checkpoint.inFlightCallID == nil,
           let todo = try await store.todoForDelivery(
            ownerUserID: ownerUserID,
            deliveryID: currentDelivery.id
           ), todo.status == .pending {
            _ = try await store.updateAgentTodo(
                ownerUserID: ownerUserID,
                agentID: todo.agentID,
                todoID: todo.id,
                update: .init(status: .inProgress, blockedReason: ""),
                nowUnixMs: now()
            )
            isRecoveryEligible = true
        }
        guard isRecoveryEligible else { return nil }

        let automaticRecoveryCount = Self.consecutiveAutomaticRecoveryCount(currentRun.events)
        if automaticRecoveryCount >= Self.maximumConsecutiveAutomaticRecoveries {
            let detail = "该任务连续 \(automaticRecoveryCount) 次在自动恢复期间中断，已只终止本任务，避免反复拉起影响其他会话。"
            var failedRun = currentRun
            failedRun.checkpoint.status = .failed
            failedRun.checkpoint.stopReason = detail
            failedRun.events.append(.init(
                kind: Self.automaticRecoveryCircuitOpenEventKind,
                detail: detail,
                modelCalls: failedRun.checkpoint.modelCalls
            ))
            failedRun.updatedAtUnixMs = max(now(), failedRun.updatedAtUnixMs)
            try await store.saveRun(failedRun)
            try await failDeliveryAndNotifyManager(
                store: store,
                ownerUserID: ownerUserID,
                delivery: currentDelivery,
                runID: currentRun.context.runID,
                detail: detail,
                stage: "automatic_recovery_circuit_open"
            )
            await service.publishChange(.init(
                ownerUserID: ownerUserID,
                roomID: currentRun.context.roomID,
                agentID: currentRun.context.agentID,
                runID: currentRun.id,
                kind: .runUpdated
            ))
            return .init(
                deliveryID: currentDelivery.id,
                agentID: currentDelivery.targetAgentID,
                outcome: .failed,
                detail: detail
            )
        }

        // Persist the attempt before launching another Worker. If the process is killed again,
        // the next app launch observes the durable count and eventually opens a circuit for only
        // this Delivery. A successful/terminal result needs no counter reset because this Run is
        // no longer selected by the interrupted-run query; an explicit Human retry is a boundary.
        var recoveringRun = currentRun
        recoveringRun.events.append(.init(
            kind: Self.automaticRecoveryStartedEventKind,
            detail: "自动恢复第 \(automaticRecoveryCount + 1) 次。",
            modelCalls: recoveringRun.checkpoint.modelCalls
        ))
        recoveringRun.updatedAtUnixMs = max(now(), recoveringRun.updatedAtUnixMs)
        try await store.saveRun(recoveringRun)
        await service.publishChange(.init(
            ownerUserID: ownerUserID,
            roomID: recoveringRun.context.roomID,
            agentID: recoveringRun.context.agentID,
            runID: recoveringRun.id,
            kind: .runUpdated
        ))
        return try await resumeRegisteredDelivery(
            ownerUserID: ownerUserID,
            projectID: currentRun.context.projectID,
            deliveryID: deliveryID
        )
    }

    static let automaticRecoveryStartedEventKind = "automatic_recovery_started"
    static let automaticRecoveryCircuitOpenEventKind = "automatic_recovery_circuit_open"
    static let maximumConsecutiveAutomaticRecoveries = 3

    enum InterruptedCheckpointDisposition: Sendable, Equatable {
        case safeRetry
        case explicitFailure
        case needsReview
    }

    static func interruptedCheckpointDisposition(
        _ checkpoint: AgentRunCheckpoint
    ) -> InterruptedCheckpointDisposition {
        if checkpoint.inFlightCallID != nil || checkpoint.status == .needsReview {
            return .needsReview
        }
        if checkpoint.status == .ready || checkpoint.status == .running {
            return .safeRetry
        }
        guard checkpoint.status == .paused,
              checkpoint.pendingCalls.isEmpty,
              let reason = checkpoint.stopReason,
              [
                  AgentContextError.unavailable.localizedDescription,
                  AgentContextError.syncUncertain.localizedDescription,
              ].contains(reason) else {
            return .explicitFailure
        }
        return .safeRetry
    }

    static func consecutiveAutomaticRecoveryCount(_ events: [AgentRunEvent]) -> Int {
        let boundary = events.lastIndex(where: { event in
            event.kind == "retry_authorized" || event.kind == automaticRecoveryCircuitOpenEventKind
        })
        let start = boundary.map { events.index(after: $0) } ?? events.startIndex
        return events[start...]
            .filter { $0.kind == automaticRecoveryStartedEventKind }
            .count
    }

    static func isAutomaticTriggerRecoveryEligible(
        _ checkpoint: AgentRunCheckpoint
    ) -> Bool {
        // Claiming a Delivery and persisting its initial Run are deliberately durable before the
        // first model request starts. If the app exits in that narrow window, the Delivery is
        // already `running` while the checkpoint is still `ready`. Replaying that checkpoint is
        // side-effect free and is required to keep the manager/executor lane from being occupied
        // forever after restart.
        interruptedCheckpointDisposition(checkpoint) == .safeRetry
    }

    func drain(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        room: ProjectAgentRoom,
        maximumRuns: Int,
        lane: LocalAgentRunLane? = nil
    ) async throws -> [DeliveryAttemptReceipt] {
        guard maximumRuns > 0,
              try await store.hasPendingDeliveries(
                  ownerUserID: ownerUserID,
                  roomID: room.id,
                  lane: lane
              ) else {
            return []
        }
        let executionPriority: LocalAgentExecutionPriority = lane == .manager
            ? .interactive
            : .background
        var members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
        return try await withThrowingTaskGroup(
            of: ScheduledWorkCompletion.self,
            returning: [DeliveryAttemptReceipt].self
        ) { group in
            var completed: [OrderedDeliveryAttemptReceipt] = []
            var inFlightCount = 0
            var nextOrder = 0
            var nextMemberIndex = 0
            var shouldSchedule = true
            var firstError: Error?

            while inFlightCount > 0
                    || (shouldSchedule
                        && !Task.isCancelled
                        && completed.count < maximumRuns) {
                var exhaustedAgentIDs = Set<String>()
                while shouldSchedule,
                      !Task.isCancelled,
                      !members.isEmpty,
                      exhaustedAgentIDs.count < members.count,
                      completed.count + inFlightCount < maximumRuns {
                    let member = members[nextMemberIndex % members.count]
                    nextMemberIndex = (nextMemberIndex + 1) % members.count
                    guard !exhaustedAgentIDs.contains(member.agentID) else { continue }
                    do {
                        try await runtime.checkExecutionAdmission()
                    } catch {
                        if firstError == nil { firstError = error }
                        shouldSchedule = false
                        break
                    }
                    let executionPermit: LocalAgentExecutionPermit?
                    if inFlightCount == 0 {
                        executionPermit = await executionGovernor.acquire(
                            priority: executionPriority
                        )
                    } else {
                        executionPermit = await executionGovernor.tryAcquire()
                    }
                    guard let executionPermit else { break }
                    if Task.isCancelled {
                        await executionPermit.releaseAndWait()
                        shouldSchedule = false
                        break
                    }
                    let delivery: ProjectAgentDelivery?
                    do {
                        if let lane {
                            delivery = try await store.claimNextDelivery(
                                ownerUserID: ownerUserID,
                                roomID: room.id,
                                agentID: member.agentID,
                                lane: lane,
                                nowUnixMs: now()
                            )
                        } else {
                            delivery = try await store.claimNextDelivery(
                                ownerUserID: ownerUserID,
                                roomID: room.id,
                                agentID: member.agentID,
                                nowUnixMs: now()
                            )
                        }
                    } catch {
                        await executionPermit.releaseAndWait()
                        if firstError == nil { firstError = error }
                        shouldSchedule = false
                        break
                    }
                    guard let delivery else {
                        await executionPermit.releaseAndWait()
                        exhaustedAgentIDs.insert(member.agentID)
                        continue
                    }
                    let work = ClaimedWork(
                        order: nextOrder,
                        room: room,
                        member: member,
                        delivery: delivery,
                        executionPermit: executionPermit
                    )
                    nextOrder += 1
                    do {
                        guard let prepared = try await prepareClaimedWork(
                            work,
                            store: store,
                            ownerUserID: ownerUserID
                        ) else {
                            continue
                        }
                        inFlightCount += 1
                        group.addTask {
                            .finished(await runPreparedClaimedWork(
                                prepared,
                                store: store,
                                ownerUserID: ownerUserID
                            ))
                        }
                    } catch {
                        if firstError == nil { firstError = error }
                        shouldSchedule = false
                        break
                    }
                }

                guard inFlightCount > 0 else { break }
                guard let completion = try await group.next() else { break }
                inFlightCount -= 1
                switch completion {
                case let .finished(receipt):
                    if let receipt { completed.append(receipt) }
                case let .failed(error):
                    if firstError == nil { firstError = error }
                    shouldSchedule = false
                }
                if shouldSchedule, !Task.isCancelled {
                    do {
                        guard try await store.hasPendingDeliveries(
                            ownerUserID: ownerUserID,
                            roomID: room.id,
                            lane: lane
                        ) else {
                            shouldSchedule = false
                            continue
                        }
                        members = try await store.listMembers(
                            ownerUserID: ownerUserID,
                            roomID: room.id
                        )
                        if !members.isEmpty {
                            nextMemberIndex %= members.count
                        } else {
                            nextMemberIndex = 0
                        }
                    } catch {
                        if firstError == nil { firstError = error }
                        shouldSchedule = false
                    }
                }
            }

            if let firstError { throw firstError }
            completed.sort { $0.order < $1.order }
            return completed.map(\.receipt)
        }
    }

    private struct PreparedClaimedWork: Sendable {
        let work: ClaimedWork
        let todoID: String?
        let cancellationHandle: LocalAgentExecutorCancellationHandle?
    }

    /// Registers scheduler ownership and executor cancellation before any model task starts. A
    /// manager launched in the same rolling-window fill can therefore cancel its executor even if
    /// the manager reaches `todo_update` before the executor child receives CPU time.
    private func prepareClaimedWork(
        _ work: ClaimedWork,
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String
    ) async throws -> PreparedClaimedWork? {
        do {
            let todoID: String?
            if work.delivery.lane == .executor,
               let candidateTodoID = work.delivery.todoID {
                let todos = try await store.todos(
                    ownerUserID: ownerUserID,
                    todoIDs: [candidateTodoID]
                )
                todoID = todos[candidateTodoID]?.agentID == work.delivery.targetAgentID
                    ? candidateTodoID
                    : nil
            } else {
                todoID = nil
            }
            let handle = todoID == nil ? nil : LocalAgentExecutorCancellationHandle()
            guard await activeDeliveryRegistry.acquire(deliveryID: work.delivery.id) else {
                await work.executionPermit.releaseAndWait()
                return nil
            }
            if let todoID, let handle {
                await executorTaskRegistry.register(todoID: todoID, handle: handle)
            }
            return .init(work: work, todoID: todoID, cancellationHandle: handle)
        } catch {
            await work.executionPermit.releaseAndWait()
            throw error
        }
    }

    private func runPreparedClaimedWork(
        _ prepared: PreparedClaimedWork,
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String
    ) async -> OrderedDeliveryAttemptReceipt {
        let work = prepared.work
        let task: Task<OrderedDeliveryAttemptReceipt, Never> = Task {
            let receipt: DeliveryAttemptReceipt
            do {
                receipt = try await runClaimedDeliveryHandlingFailure(
                    store: store,
                    ownerUserID: ownerUserID,
                    projectID: work.room.projectID,
                    room: work.room,
                    member: work.member,
                    delivery: work.delivery
                )
            } catch {
                receipt = .init(
                    deliveryID: work.delivery.id,
                    agentID: work.delivery.targetAgentID,
                    outcome: .failed,
                    detail: Self.failureDetail(error)
                )
            }
            // The failure wrapper itself can fail while persisting its terminal state. Keep
            // invalidation outside that wrapper so a visible room always reconciles durable
            // state after this claimed attempt leaves the scheduler.
            await service.publishChange(.init(
                ownerUserID: ownerUserID,
                roomID: work.room.id,
                agentID: work.delivery.targetAgentID,
                kind: .roomUpdated
            ))
            await work.executionPermit.releaseAndWait()
            return OrderedDeliveryAttemptReceipt(order: work.order, receipt: receipt)
        }
        prepared.cancellationHandle?.install { task.cancel() }
        let completed = await task.value
        await activeDeliveryRegistry.unregister(deliveryID: completed.receipt.deliveryID)
        if let todoID = prepared.todoID,
           let handle = prepared.cancellationHandle {
            await executorTaskRegistry.unregister(todoID: todoID, handle: handle)
        }
        return completed
    }

    static func memberKey(roomID: String, agentID: String) -> String {
        "\(roomID)\u{0}\(agentID)"
    }
}
