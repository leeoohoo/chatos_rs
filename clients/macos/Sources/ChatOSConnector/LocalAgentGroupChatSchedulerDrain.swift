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

    func drainAccountQueue(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        maximumRuns: Int,
        lane: LocalAgentRunLane?
    ) async throws -> [DeliveryAttemptReceipt] {
        let executionPriority: LocalAgentExecutionPriority = lane == .manager
            ? .interactive
            : .background
        var results: [DeliveryAttemptReceipt] = []
        while results.count < maximumRuns, !Task.isCancelled {
            guard try await store.hasPendingDeliveries(
                ownerUserID: ownerUserID,
                lane: lane
            ) else { break }
            let teams = try await store.listRooms(ownerUserID: ownerUserID, includeArchived: false)
            let directs = try await store.listDirectConversations(
                ownerUserID: ownerUserID,
                includeArchived: false
            )
            let rooms = teams + directs
            let remainingCapacity = maximumRuns - results.count
            var roomByID = Dictionary(uniqueKeysWithValues: rooms.map { ($0.id, $0) })
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
            var claimedWork: [ClaimedWork] = []
            claimedWork.reserveCapacity(remainingCapacity)
            var exhaustedAgentIDs = Set<String>()
            // Claim in round-robin passes. A single Agent may own independent lanes in different
            // rooms, but it must not consume every available permit before other Agents get one.
            claimingPasses: while claimedWork.count < remainingCapacity {
                var claimedInPass = false
                for agentID in agentIDs where !exhaustedAgentIDs.contains(agentID) {
                    try await runtime.checkExecutionAdmission()
                    let executionPermit: LocalAgentExecutionPermit?
                    if claimedWork.isEmpty {
                        executionPermit = await executionGovernor.acquire(
                            priority: executionPriority
                        )
                    } else {
                        executionPermit = await executionGovernor.tryAcquire()
                    }
                    guard let executionPermit else { break claimingPasses }
                    if Task.isCancelled {
                        await executionPermit.releaseAndWait()
                        break claimingPasses
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
                        throw error
                    }
                    guard let delivery else {
                        await executionPermit.releaseAndWait()
                        exhaustedAgentIDs.insert(agentID)
                        continue
                    }
                    do {
                        let room: ProjectAgentRoom
                        if let known = roomByID[delivery.roomID] {
                            room = known
                        } else if let loaded = try await store.room(
                            ownerUserID: ownerUserID,
                            roomID: delivery.roomID
                        ) {
                            room = loaded
                            roomByID[loaded.id] = loaded
                        } else {
                            throw AgentGroupChatError.notFound
                        }
                        let key = Self.memberKey(roomID: room.id, agentID: agentID)
                        let member: ProjectAgentRoomMember
                        if let known = memberByRoomAndAgent[key] {
                            member = known
                        } else if let loaded = try await store.listMembers(
                            ownerUserID: ownerUserID,
                            roomID: room.id
                        ).first(where: { $0.agentID == agentID && $0.status == .active }) {
                            member = loaded
                            memberByRoomAndAgent[key] = loaded
                        } else {
                            throw AgentGroupChatError.notMember
                        }
                        claimedWork.append(.init(
                            order: claimedWork.count,
                            room: room,
                            member: member,
                            delivery: delivery,
                            executionPermit: executionPermit
                        ))
                        claimedInPass = true
                    } catch {
                        await executionPermit.releaseAndWait()
                        throw error
                    }
                    if claimedWork.count == remainingCapacity { break }
                }
                if !claimedInPass { break }
            }
            if claimedWork.isEmpty { break }
            results.append(contentsOf: try await runClaimedWork(
                claimedWork,
                store: store,
                ownerUserID: ownerUserID
            ))
        }
        return results
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
        var results: [DeliveryAttemptReceipt] = []
        var nextRunIndex = 0
        while nextRunIndex < runs.count,
              results.count < maximumRuns,
              !Task.isCancelled {
            let batchStartIndex = nextRunIndex
            let remainingCapacity = maximumRuns - results.count
            var running: [Task<OrderedDeliveryAttemptReceipt?, Error>] = []
            running.reserveCapacity(min(remainingCapacity, executionGovernor.limit))
            while nextRunIndex < runs.count, running.count < remainingCapacity {
                try await runtime.checkExecutionAdmission()
                let executionPermit: LocalAgentExecutionPermit?
                if running.isEmpty {
                    executionPermit = await executionGovernor.acquire(priority: .background)
                } else {
                    executionPermit = await executionGovernor.tryAcquire()
                }
                guard let executionPermit else { break }
                if Task.isCancelled {
                    await executionPermit.releaseAndWait()
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
                running.append(Task {
                    do {
                        let receipt = try await recoverInterruptedRun(
                            store: store,
                            ownerUserID: ownerUserID,
                            run: run
                        )
                        await activeDeliveryRegistry.unregister(deliveryID: deliveryID)
                        await executionPermit.releaseAndWait()
                        return receipt.map {
                            OrderedDeliveryAttemptReceipt(order: order, receipt: $0)
                        }
                    } catch {
                        await activeDeliveryRegistry.unregister(deliveryID: deliveryID)
                        await executionPermit.releaseAndWait()
                        throw error
                    }
                })
            }
            guard !running.isEmpty else {
                if nextRunIndex > batchStartIndex { continue }
                break
            }

            var batch: [OrderedDeliveryAttemptReceipt] = []
            var firstError: Error?
            for task in running {
                do {
                    if let completed = try await task.value {
                        batch.append(completed)
                    }
                } catch {
                    if firstError == nil { firstError = error }
                }
            }
            if let firstError { throw firstError }
            batch.sort { $0.order < $1.order }
            results.append(contentsOf: batch.map(\.receipt))
            if batch.contains(where: { $0.receipt.outcome != .completed }) { break }
        }
        return results
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
        if currentRun.checkpoint.status == .needsReview {
            try await suspendTodoForReview(
                store: store,
                ownerUserID: ownerUserID,
                delivery: currentDelivery,
                runID: currentRun.context.runID,
                detail: currentRun.checkpoint.stopReason
                    ?? "执行中断，需要检查副作用后再决定是否重试。"
            )
            return nil
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
        return try await resumeRegisteredDelivery(
            ownerUserID: ownerUserID,
            projectID: currentRun.context.projectID,
            deliveryID: deliveryID
        )
    }

    static func isAutomaticTriggerRecoveryEligible(
        _ checkpoint: AgentRunCheckpoint
    ) -> Bool {
        // Claiming a Delivery and persisting its initial Run are deliberately durable before the
        // first model request starts. If the app exits in that narrow window, the Delivery is
        // already `running` while the checkpoint is still `ready`. Replaying that checkpoint is
        // side-effect free and is required to keep the manager/executor lane from being occupied
        // forever after restart.
        if checkpoint.status == .ready || checkpoint.status == .running { return true }
        guard checkpoint.status == .paused,
              checkpoint.pendingCalls.isEmpty,
              checkpoint.inFlightCallID == nil,
              let reason = checkpoint.stopReason else { return false }
        return [
            AgentContextError.unavailable.localizedDescription,
            AgentContextError.syncUncertain.localizedDescription,
        ].contains(reason)
    }

    func drain(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        room: ProjectAgentRoom,
        maximumRuns: Int,
        lane: LocalAgentRunLane? = nil
    ) async throws -> [DeliveryAttemptReceipt] {
        let executionPriority: LocalAgentExecutionPriority = lane == .manager
            ? .interactive
            : .background
        var results: [DeliveryAttemptReceipt] = []
        while results.count < maximumRuns {
            if Task.isCancelled { break }
            guard try await store.hasPendingDeliveries(
                ownerUserID: ownerUserID,
                roomID: room.id,
                lane: lane
            ) else { break }
            let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
            let remainingCapacity = maximumRuns - results.count
            var claimedWork: [ClaimedWork] = []
            claimedWork.reserveCapacity(min(remainingCapacity, members.count * 2))
            claimingMembers: for member in members where claimedWork.count < remainingCapacity {
                for _ in 0..<2 where claimedWork.count < remainingCapacity {
                    if Task.isCancelled { break }
                    try await runtime.checkExecutionAdmission()
                    let executionPermit: LocalAgentExecutionPermit?
                    if claimedWork.isEmpty {
                        executionPermit = await executionGovernor.acquire(
                            priority: executionPriority
                        )
                    } else {
                        executionPermit = await executionGovernor.tryAcquire()
                    }
                    guard let executionPermit else { break claimingMembers }
                    if Task.isCancelled {
                        await executionPermit.releaseAndWait()
                        break claimingMembers
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
                        throw error
                    }
                    guard let delivery else {
                        await executionPermit.releaseAndWait()
                        break
                    }
                    claimedWork.append(.init(
                        order: claimedWork.count,
                        room: room,
                        member: member,
                        delivery: delivery,
                        executionPermit: executionPermit
                    ))
                }
            }
            if claimedWork.isEmpty { break }
            results.append(contentsOf: try await runClaimedWork(
                claimedWork,
                store: store,
                ownerUserID: ownerUserID
            ))
        }
        return results
    }

    func runClaimedWork(
        _ claimedWork: [ClaimedWork],
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String
    ) async throws -> [DeliveryAttemptReceipt] {
        var running: [(task: Task<OrderedDeliveryAttemptReceipt, Never>, todoID: String?, handle: LocalAgentExecutorCancellationHandle?)] = []
        running.reserveCapacity(claimedWork.count)
        // Register executor handles before manager tasks can invoke todo_update(status=cancelled).
        let launchOrder = claimedWork.sorted {
            if $0.delivery.lane != $1.delivery.lane {
                return $0.delivery.lane == .executor
            }
            return $0.order < $1.order
        }
        let executorTodoIDsByDeliveryID = Dictionary(uniqueKeysWithValues: launchOrder.compactMap {
            work -> (String, String)? in
            guard work.delivery.lane == .executor,
                  let todoID = work.delivery.todoID else { return nil }
            return (work.delivery.id, todoID)
        })
        let executorTodos = try await store.todos(
            ownerUserID: ownerUserID,
            todoIDs: Array(executorTodoIDsByDeliveryID.values)
        )
        for work in launchOrder {
            let todoID: String? = executorTodoIDsByDeliveryID[work.delivery.id].flatMap { todoID in
                guard executorTodos[todoID]?.agentID == work.delivery.targetAgentID else {
                    return nil
                }
                return todoID
            }
            let handle = todoID == nil ? nil : LocalAgentExecutorCancellationHandle()
            guard await activeDeliveryRegistry.acquire(deliveryID: work.delivery.id) else {
                await work.executionPermit.releaseAndWait()
                continue
            }
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
            handle?.install { task.cancel() }
            if let todoID, let handle {
                await executorTaskRegistry.register(todoID: todoID, handle: handle)
            }
            running.append((task, todoID, handle))
        }
        var round: [OrderedDeliveryAttemptReceipt] = []
        round.reserveCapacity(running.count)
        for entry in running {
            let completed = await entry.task.value
            round.append(completed)
            await activeDeliveryRegistry.unregister(deliveryID: completed.receipt.deliveryID)
            if let todoID = entry.todoID, let handle = entry.handle {
                await executorTaskRegistry.unregister(todoID: todoID, handle: handle)
            }
        }
        round.sort { $0.order < $1.order }
        return round.map(\.receipt)
    }

    static func memberKey(roomID: String, agentID: String) -> String {
        "\(roomID)\u{0}\(agentID)"
    }
}
