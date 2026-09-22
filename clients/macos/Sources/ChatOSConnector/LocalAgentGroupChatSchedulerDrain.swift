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
        guard await accountDrainCoordinator.acquire(ownerUserID: ownerUserID) else {
            throw CancellationError()
        }
        do {
            try Task.checkCancellation()
            let results = try await drainAccountWithLease(
                ownerUserID: ownerUserID,
                maximumRuns: maximumRuns
            )
            await accountDrainCoordinator.release(ownerUserID: ownerUserID)
            return results
        } catch {
            await accountDrainCoordinator.release(ownerUserID: ownerUserID)
            throw error
        }
    }

    func drainAccountWithLease(
        ownerUserID: String,
        maximumRuns: Int
    ) async throws -> [DeliveryAttemptReceipt] {
        let store = try await service.store()
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
        var results: [DeliveryAttemptReceipt] = []
        while results.count < maximumRuns, !Task.isCancelled {
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
            for room in rooms {
                for member in try await store.listMembers(
                    ownerUserID: ownerUserID,
                    roomID: room.id
                ) where member.status == .active {
                    memberByRoomAndAgent[Self.memberKey(
                        roomID: room.id,
                        agentID: member.agentID
                    )] = member
                    if seenAgentIDs.insert(member.agentID).inserted {
                        agentIDs.append(member.agentID)
                    }
                }
            }
            var claimedWork: [ClaimedWork] = []
            claimedWork.reserveCapacity(remainingCapacity)
            for agentID in agentIDs where claimedWork.count < remainingCapacity {
                // A full drain may claim both independent lanes for one Agent. A lane-specific
                // drain claims once so the same manager or executor lane remains serialized.
                let claimsPerAgent = lane == nil ? 2 : 1
                for _ in 0..<claimsPerAgent where claimedWork.count < remainingCapacity {
                    let delivery: ProjectAgentDelivery?
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
                    guard let delivery else { break }
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
                        delivery: delivery
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

    /// Chat surfaces only append/display messages. Recovery belongs to the Agent trigger runtime.
    /// The account lease serializes recovery passes, while the active-delivery registry excludes
    /// live communication work that intentionally runs beside a long executor task. Any remaining
    /// durable `running` checkpoint was left behind by an app exit or interrupted provider request.
    /// Resuming is safe even with an in-flight write marker because AgentRuntime converts that
    /// checkpoint to `needsReview` before any replay. User pauses, limits, and review states remain
    /// untouched.
    func recoverInterruptedRuns(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        maximumRuns: Int
    ) async throws -> [DeliveryAttemptReceipt] {
        let agents = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: false)
        var results: [DeliveryAttemptReceipt] = []
        for agent in agents where results.count < maximumRuns {
            let runs = try await store.listAgentRuns(
                ownerUserID: ownerUserID,
                agentID: agent.id,
                limit: 20
            )
            for run in runs where results.count < maximumRuns {
                guard Self.isAutomaticTriggerRecoveryEligible(run.checkpoint),
                      let delivery = try await store.delivery(
                        ownerUserID: ownerUserID,
                        deliveryID: run.context.deliveryID
                      ), delivery.status == .running else { continue }
                guard !(await activeDeliveryRegistry.contains(deliveryID: delivery.id)) else {
                    continue
                }
                let result = try await resumeDelivery(
                    ownerUserID: ownerUserID,
                    projectID: run.context.projectID,
                    deliveryID: delivery.id
                )
                results.append(result)
                if result.outcome != .completed { break }
            }
        }
        return results
    }

    static func isAutomaticTriggerRecoveryEligible(
        _ checkpoint: AgentRunCheckpoint
    ) -> Bool {
        if checkpoint.status == .running { return true }
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
        var results: [DeliveryAttemptReceipt] = []
        while results.count < maximumRuns {
            if Task.isCancelled { break }
            let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
            let remainingCapacity = maximumRuns - results.count
            var claimedWork: [ClaimedWork] = []
            claimedWork.reserveCapacity(min(remainingCapacity, members.count * 2))
            for member in members where claimedWork.count < remainingCapacity {
                for _ in 0..<2 where claimedWork.count < remainingCapacity {
                    if Task.isCancelled { break }
                    let delivery: ProjectAgentDelivery?
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
                    guard let delivery else { break }
                    claimedWork.append(.init(
                        order: claimedWork.count,
                        room: room,
                        member: member,
                        delivery: delivery
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
        for work in launchOrder {
            let todoID = work.delivery.lane == .executor
                ? try await store.todoForDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: work.delivery.id
                )?.id
                : nil
            let handle = todoID == nil ? nil : LocalAgentExecutorCancellationHandle()
            await activeDeliveryRegistry.register(deliveryID: work.delivery.id)
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
