import ChatOSAgentRuntime
import ChatOSCore
import Foundation

private final class LocalAgentExecutorCancellationHandle: @unchecked Sendable {
    private let lock = NSLock()
    private var cancellation: (@Sendable () -> Void)?
    private var cancellationRequested = false

    func install(_ cancellation: @escaping @Sendable () -> Void) {
        lock.lock()
        if cancellationRequested {
            lock.unlock()
            cancellation()
            return
        }
        self.cancellation = cancellation
        lock.unlock()
    }

    func cancel() {
        lock.lock()
        cancellationRequested = true
        let cancellation = cancellation
        lock.unlock()
        cancellation?()
    }
}

private actor LocalAgentExecutorTaskRegistry {
    private var handles: [String: LocalAgentExecutorCancellationHandle] = [:]

    func register(todoID: String, handle: LocalAgentExecutorCancellationHandle) {
        handles[todoID] = handle
    }

    func unregister(todoID: String, handle: LocalAgentExecutorCancellationHandle) {
        guard handles[todoID] === handle else { return }
        handles.removeValue(forKey: todoID)
    }

    func cancel(todoID: String) {
        handles[todoID]?.cancel()
    }
}

/// Serializes account-wide drain passes created by multiple windows and the heartbeat loop. The
/// durable SQLite queue remains authoritative; this lease only prevents two local consumers from
/// observing the same Agent lane as busy and leaving newly queued work stranded between passes.
private actor LocalAgentAccountDrainCoordinator {
    private struct Waiter {
        let id: UUID
        let continuation: CheckedContinuation<Bool, Never>
    }

    private var activeOwners: Set<String> = []
    private var waitersByOwner: [String: [Waiter]] = [:]

    func acquire(ownerUserID: String) async -> Bool {
        guard !Task.isCancelled else { return false }
        if activeOwners.insert(ownerUserID).inserted { return true }
        let waiterID = UUID()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                waitersByOwner[ownerUserID, default: []].append(.init(
                    id: waiterID,
                    continuation: continuation
                ))
            }
        } onCancel: {
            Task { await self.cancel(ownerUserID: ownerUserID, waiterID: waiterID) }
        }
    }

    func release(ownerUserID: String) {
        if var waiters = waitersByOwner[ownerUserID], !waiters.isEmpty {
            let next = waiters.removeFirst()
            if waiters.isEmpty {
                waitersByOwner.removeValue(forKey: ownerUserID)
            } else {
                waitersByOwner[ownerUserID] = waiters
            }
            next.continuation.resume(returning: true)
        } else {
            activeOwners.remove(ownerUserID)
        }
    }

    private func cancel(ownerUserID: String, waiterID: UUID) {
        guard var waiters = waitersByOwner[ownerUserID],
              let index = waiters.firstIndex(where: { $0.id == waiterID }) else { return }
        let waiter = waiters.remove(at: index)
        if waiters.isEmpty {
            waitersByOwner.removeValue(forKey: ownerUserID)
        } else {
            waitersByOwner[ownerUserID] = waiters
        }
        waiter.continuation.resume(returning: false)
    }
}

/// Consumes the durable room delivery queue entirely in the macOS client. The server is used
/// only through `AgentServiceProviding` for account-owned model credentials and Memory Engine;
/// it never selects an Agent, routes a message, or owns a run checkpoint.
public struct LocalAgentGroupChatScheduler: Sendable {
    public enum RunOutcome: String, Sendable, Equatable {
        case completed
        case suspended
        case failed
    }

    public struct RunResult: Sendable {
        public let deliveryID: String
        public let agentID: String
        public let outcome: RunOutcome
        public let detail: String?
    }

    private struct ClaimedWork: Sendable {
        let order: Int
        let room: ProjectAgentRoom
        let member: ProjectAgentRoomMember
        let delivery: ProjectAgentDelivery
    }

    private struct OrderedRunResult: Sendable {
        let order: Int
        let result: RunResult
    }

    public typealias AdditionalToolProviderFactory = @Sendable (
        _ profile: LocalAgentProfile,
        _ member: ProjectAgentRoomMember,
        _ context: LocalAgentChatRunContext
    ) async throws -> [any AgentToolProvider]
    public typealias ProjectTypeKeyProvider = @Sendable (
        _ ownerUserID: String,
        _ projectID: String
    ) async throws -> String?
    public typealias ProfessionProvider = @Sendable (
        _ ownerUserID: String,
        _ professionKey: String
    ) async throws -> LocalAgentProfessionDefinition?
    public typealias ProjectTypeProvider = @Sendable (
        _ ownerUserID: String,
        _ projectTypeKey: String
    ) async throws -> LocalProjectTypeDefinition?
    public typealias ProfessionCatalogProvider = @Sendable (
        _ ownerUserID: String
    ) async throws -> [LocalAgentProfessionDefinition]
    public typealias TodoPluginCatalogProvider = @Sendable (
        _ ownerUserID: String
    ) async throws -> [LocalAgentTodoPluginOption]
    public typealias ContextLanguageProvider = @Sendable (
        _ ownerUserID: String
    ) async -> ChatOSLanguage

    private let service: NativeAgentGroupChatService
    private let services: any AgentServiceProviding
    private let settings: AgentSettingsStore
    private let runtime: AgentRuntime
    private let limits: AgentGroupChatRoutingLimits
    private let relayMCP: LocalAgentRelayMCPServer
    private let executorTaskRegistry: LocalAgentExecutorTaskRegistry
    private let accountDrainCoordinator: LocalAgentAccountDrainCoordinator
    private let additionalToolProviders: AdditionalToolProviderFactory
    private let projectTypeKeyProvider: ProjectTypeKeyProvider
    private let professionProvider: ProfessionProvider
    private let projectTypeProvider: ProjectTypeProvider
    private let professionCatalogProvider: ProfessionCatalogProvider
    private let todoPluginCatalogProvider: TodoPluginCatalogProvider
    private let contextLanguageProvider: ContextLanguageProvider
    private let now: @Sendable () -> Int64

    public init(
        service: NativeAgentGroupChatService,
        services: any AgentServiceProviding,
        settings: AgentSettingsStore = .init(),
        runtime: AgentRuntime = .init(),
        limits: AgentGroupChatRoutingLimits = .init(),
        projectTypeKeyProvider: @escaping ProjectTypeKeyProvider = { _, _ in nil },
        professionProvider: @escaping ProfessionProvider = { _, key in
            LocalAgentSkillCatalog.profession(key: key)
        },
        projectTypeProvider: @escaping ProjectTypeProvider = { _, key in
            LocalAgentSkillCatalog.projectType(key: key)
        },
        professionCatalogProvider: @escaping ProfessionCatalogProvider = { _ in
            LocalAgentSkillCatalog.professions
        },
        todoPluginCatalogProvider: @escaping TodoPluginCatalogProvider = { _ in [] },
        contextLanguageProvider: @escaping ContextLanguageProvider = { _ in .simplifiedChinese },
        additionalToolProviders: @escaping AdditionalToolProviderFactory = { _, _, _ in [] },
        now: @escaping @Sendable () -> Int64 = {
            Int64(Date().timeIntervalSince1970 * 1_000)
        }
    ) {
        self.service = service
        self.services = services
        self.settings = settings
        self.runtime = runtime
        self.limits = limits
        let executorTaskRegistry = LocalAgentExecutorTaskRegistry()
        self.executorTaskRegistry = executorTaskRegistry
        self.accountDrainCoordinator = LocalAgentAccountDrainCoordinator()
        self.relayMCP = LocalAgentRelayMCPServer(
            service: service,
            limits: limits,
            todoCancellationHandler: { todoID in
                await executorTaskRegistry.cancel(todoID: todoID)
            },
            now: now
        )
        self.projectTypeKeyProvider = projectTypeKeyProvider
        self.professionProvider = professionProvider
        self.projectTypeProvider = projectTypeProvider
        self.professionCatalogProvider = professionCatalogProvider
        self.todoPluginCatalogProvider = todoPluginCatalogProvider
        self.contextLanguageProvider = contextLanguageProvider
        self.additionalToolProviders = additionalToolProviders
        self.now = now
    }

    /// Drains all currently reachable deliveries in one project. Each round first claims at most
    /// one delivery per Agent, then runs different Agents concurrently. Waiting for the round to
    /// finish before claiming again preserves per-Agent serialization while re-reading the member
    /// queue lets an Agent's `@mention` wake another Agent without server polling.
    public func drainProject(
        ownerUserID: String,
        projectID: String,
        maximumRuns: Int = 32
    ) async throws -> [RunResult] {
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
    ) async throws -> [RunResult] {
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

    /// Drains all account-owned conversations so a Relay message that opens another private
    /// conversation is delivered without requiring that destination to be visible in the UI.
    public func drainAccount(
        ownerUserID: String,
        maximumRuns: Int = 64
    ) async throws -> [RunResult] {
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

    private func drainAccountWithLease(
        ownerUserID: String,
        maximumRuns: Int
    ) async throws -> [RunResult] {
        let store = try await service.store()
        var results = try await recoverInterruptedRuns(
            store: store,
            ownerUserID: ownerUserID,
            maximumRuns: maximumRuns
        )
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
                // A single Agent owns two independent durable lanes. Claiming twice lets its
                // manager keep receiving messages while one project-bound executor is working.
                for _ in 0..<2 where claimedWork.count < remainingCapacity {
                    guard let delivery = try await store.claimNextDelivery(
                        ownerUserID: ownerUserID,
                        agentID: agentID,
                        nowUnixMs: now()
                    ) else { break }
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
    /// A durable `running` checkpoint cannot belong to a live run here because this account drain
    /// holds the process-wide lease; it was left behind by an app exit or an interrupted provider
    /// request. Resuming is safe even with an in-flight write marker because AgentRuntime converts
    /// that checkpoint to `needsReview` before any replay. User pauses, limits, and review states
    /// remain untouched.
    private func recoverInterruptedRuns(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        maximumRuns: Int
    ) async throws -> [RunResult] {
        let agents = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: false)
        var results: [RunResult] = []
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
            AgentContextError.invalidHistory.localizedDescription,
            AgentContextError.syncUncertain.localizedDescription,
        ].contains(reason)
    }

    private func drain(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        room: ProjectAgentRoom,
        maximumRuns: Int
    ) async throws -> [RunResult] {
        var results: [RunResult] = []
        while results.count < maximumRuns {
            if Task.isCancelled { break }
            let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
            let remainingCapacity = maximumRuns - results.count
            var claimedWork: [ClaimedWork] = []
            claimedWork.reserveCapacity(min(remainingCapacity, members.count * 2))
            for member in members where claimedWork.count < remainingCapacity {
                for _ in 0..<2 where claimedWork.count < remainingCapacity {
                    if Task.isCancelled { break }
                    guard let delivery = try await store.claimNextDelivery(
                        ownerUserID: ownerUserID,
                        roomID: room.id,
                        agentID: member.agentID,
                        nowUnixMs: now()
                    ) else { break }
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

    private func runClaimedWork(
        _ claimedWork: [ClaimedWork],
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String
    ) async throws -> [RunResult] {
        var running: [(task: Task<OrderedRunResult, Never>, todoID: String?, handle: LocalAgentExecutorCancellationHandle?)] = []
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
            let task: Task<OrderedRunResult, Never> = Task {
                let result: RunResult
                do {
                    result = try await runClaimedDeliveryHandlingFailure(
                        store: store,
                        ownerUserID: ownerUserID,
                        projectID: work.room.projectID,
                        room: work.room,
                        member: work.member,
                        delivery: work.delivery
                    )
                } catch {
                    result = .init(
                        deliveryID: work.delivery.id,
                        agentID: work.delivery.targetAgentID,
                        outcome: .failed,
                        detail: Self.failureDetail(error)
                    )
                }
                return OrderedRunResult(order: work.order, result: result)
            }
            handle?.install { task.cancel() }
            if let todoID, let handle {
                await executorTaskRegistry.register(todoID: todoID, handle: handle)
            }
            running.append((task, todoID, handle))
        }
        var round: [OrderedRunResult] = []
        round.reserveCapacity(running.count)
        for entry in running {
            round.append(await entry.task.value)
            if let todoID = entry.todoID, let handle = entry.handle {
                await executorTaskRegistry.unregister(todoID: todoID, handle: handle)
            }
        }
        round.sort { $0.order < $1.order }
        return round.map(\.result)
    }

    private static func memberKey(roomID: String, agentID: String) -> String {
        "\(roomID)\u{0}\(agentID)"
    }

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
    ) async throws -> RunResult {
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
    ) async throws -> RunResult {
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
        savedRun.events.append(.init(
            kind: "retry_authorized",
            detail: "Human 已明确重试中断步骤：\(inFlightCallID)",
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

    private func runClaimedDeliveryHandlingFailure(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        projectID: String,
        room: ProjectAgentRoom,
        member: ProjectAgentRoomMember,
        delivery: ProjectAgentDelivery
    ) async throws -> RunResult {
        do {
            return try await runClaimedDelivery(
                store: store,
                ownerUserID: ownerUserID,
                projectID: projectID,
                room: room,
                member: member,
                delivery: delivery
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

    private func runClaimedDelivery(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        projectID: String,
        room: ProjectAgentRoom,
        member: ProjectAgentRoomMember,
        delivery: ProjectAgentDelivery,
        savedRun: LocalAgentGroupChatRun? = nil
    ) async throws -> RunResult {
        guard room.ownerUserID == ownerUserID,
              room.projectID == projectID,
              room.id == delivery.roomID,
              member.ownerUserID == ownerUserID,
              member.roomID == room.id,
              member.agentID == delivery.targetAgentID,
              member.status == .active else {
            throw AgentGroupChatError.conflict
        }
        let profiles = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: false)
        guard let profile = profiles.first(where: { $0.id == delivery.targetAgentID }) else {
            throw AgentGroupChatError.notFound
        }

        let run: LocalAgentGroupChatRun
        if let savedRun {
            guard savedRun.context.ownerUserID == ownerUserID,
                  savedRun.context.projectID == projectID,
                  savedRun.context.roomID == room.id,
                  savedRun.context.agentID == profile.id,
                  savedRun.context.deliveryID == delivery.id,
                  savedRun.context.triggerMessageID == delivery.messageID,
                  savedRun.context.rootMessageID == delivery.rootMessageID,
                  savedRun.context.lane == delivery.lane,
                  savedRun.checkpoint.status != .completed else {
                throw AgentGroupChatError.conflict
            }
            run = savedRun
        } else {
            let runID = UUID()
            let context = try LocalAgentChatRunContext(
                ownerUserID: ownerUserID,
                projectID: projectID,
                roomID: room.id,
                agentID: profile.id,
                deliveryID: delivery.id,
                triggerMessageID: delivery.messageID,
                rootMessageID: delivery.rootMessageID,
                runID: runID.uuidString.lowercased(),
                hopCount: delivery.hopCount,
                lane: delivery.lane
            )
            let scope = LocalAgentGroupChatRun.runtimeScope(for: context)
            let policy = try settings.load().global
            try policy.validate()
            let selectedProfession = try await professionProvider(
                ownerUserID,
                profile.draft.professionKey
            )
            let fallbackProfession = try await professionProvider(
                ownerUserID,
                LocalAgentSkillCatalog.legacyProfessionKey
            )
            let profession = selectedProfession ?? fallbackProfession
                ?? LocalAgentSkillCatalog.profession(
                key: LocalAgentSkillCatalog.legacyProfessionKey
            )!
            let projectType: LocalProjectTypeDefinition?
            if room.conversationKind == .projectTeam {
                let projectTypeKey = try await projectTypeKeyProvider(ownerUserID, projectID)
                    ?? LocalAgentSkillCatalog.legacyProjectTypeKey
                projectType = try await projectTypeProvider(ownerUserID, projectTypeKey)
            } else {
                projectType = nil
            }
            let contextLanguage = await contextLanguageProvider(ownerUserID)
            guard let triggerMessage = try await store.message(
                ownerUserID: ownerUserID,
                roomID: room.id,
                messageID: delivery.messageID
            ) else { throw AgentGroupChatError.notFound }
            var triggerAttachments: [ProjectAgentMessageAttachmentPayload] = []
            for attachment in triggerMessage.attachmentItems {
                guard let payload = try await store.messageAttachment(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    messageID: triggerMessage.id,
                    attachmentID: attachment.id
                ) else { throw AgentGroupChatError.storage("message attachment is missing") }
                triggerAttachments.append(payload)
            }
            var initial = AgentRunCheckpoint(
                scope: scope,
                messages: Self.initialMessages(
                    profile: profile,
                    member: member,
                    room: room,
                    delivery: delivery,
                    profession: profession,
                    projectType: projectType,
                    contextLanguage: contextLanguage,
                    triggerMessage: triggerMessage,
                    triggerAttachments: triggerAttachments
                )
            )
            initial.id = runID
            let createdAt = now()
            run = try LocalAgentGroupChatRun(
                id: runID,
                context: context,
                modelConfigID: profile.draft.modelConfigID,
                policy: policy,
                checkpoint: initial,
                createdAtUnixMs: createdAt,
                updatedAtUnixMs: createdAt
            )
            try await store.saveRun(run)
            await service.publishChange(.init(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: profile.id,
                runID: run.id,
                kind: .deliveryClaimed
            ))
        }
        let runID = run.id
        let context = run.context
        let scope = run.checkpoint.scope
        let policy = run.policy
        var checkpoint = run.checkpoint
        let session = LocalAgentGroupChatRunSession(
            run: run,
            store: store,
            now: now,
            didPersist: { [service] saved in
                await service.publishChange(.init(
                    ownerUserID: saved.context.ownerUserID,
                    roomID: saved.context.roomID,
                    agentID: saved.context.agentID,
                    runID: saved.id,
                    kind: .runUpdated
                ))
            }
        )

        let executionTodo: LocalAgentTodo?
        if context.lane == .executor {
            executionTodo = try await store.todoForDelivery(
                ownerUserID: ownerUserID,
                deliveryID: delivery.id
            )
            guard executionTodo?.agentID == profile.id,
                  executionTodo?.teamRoomID == room.id else {
                throw AgentGroupChatError.conflict
            }
        } else {
            executionTodo = nil
        }

        if let todo = executionTodo, savedRun == nil {
            _ = try await store.appendAgentTodoProgress(
                ownerUserID: ownerUserID,
                agentID: profile.id,
                todoID: todo.id,
                kind: .started,
                runID: context.runID,
                stage: "started",
                detail: "Todo 执行线程已启动，并完成团队、项目和能力计划绑定。",
                nowUnixMs: now()
            )
        }

        var memoryProvider: AgentMemoryContextProvider?
        do {
            let memoryScope: AgentMemoryScope
            if let boundScope = checkpoint.memory?.scope {
                memoryScope = boundScope
            } else {
                if let executionTodo {
                    memoryScope = try AgentMemoryScope(
                        tenantID: ownerUserID,
                        todoID: executionTodo.id,
                        runID: runID,
                        runtimeScope: scope
                    )
                } else {
                    memoryScope = try AgentMemoryScope(
                        tenantID: ownerUserID,
                        agentID: profile.id,
                        projectID: projectID,
                        runID: runID,
                        runtimeScope: scope
                    )
                }
            }
            let memory = try await services.makeAgentMemory(scope: memoryScope)
            // Establish the bound thread before attaching Memory to the checkpoint. A fresh run
            // may continue without Memory when this preflight is offline; a resumed run pauses
            // instead of silently switching or dropping its already-bound Memory scope.
            try await memory.ensureThread()
            let provider = AgentMemoryContextProvider(scope: memoryScope, service: memory)
            if checkpoint.memory == nil {
                checkpoint = try provider.bind(checkpoint)
            }
            checkpoint.memory?.threadCreated = true
            memoryProvider = provider
            try await session.record(
                checkpoint: checkpoint,
                event: .init(
                    kind: savedRun == nil ? "memory_bound" : "memory_reconnected",
                    detail: context.lane == .executor
                        ? "已绑定当前 Todo 的独立执行 Memory"
                        : "已绑定当前 Agent 的长期通讯 Memory",
                    modelCalls: checkpoint.modelCalls
                )
            )
        } catch {
            // Continuity is part of the Agent identity contract. Never execute a fresh or resumed
            // delivery without its bound Memory thread: doing so makes one wake-up behave like a
            // new Agent and can produce decisions that contradict earlier work. This technical
            // pause is safe to retry because no model call or side-effect tool has started yet.
            try await session.record(
                checkpoint: checkpoint,
                event: .init(
                    kind: "memory_unavailable",
                    detail: Self.failureDetail(error),
                    modelCalls: checkpoint.modelCalls
                )
            )
            checkpoint.status = .paused
            checkpoint.stopReason = AgentContextError.unavailable.localizedDescription
            _ = try await session.finish(checkpoint: checkpoint)
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .suspended,
                detail: "当前 Agent 的连续 Memory 暂时不可用，已安全暂停并等待自动重试。"
            )
        }

        let chatProvider = try await relayMCP.connect(
            context: context,
            professions: try await professionCatalogProvider(ownerUserID),
            todoPluginOptions: context.lane == .manager
                ? try await todoPluginCatalogProvider(ownerUserID)
                : []
        )
        let extraProviders = try await additionalToolProviders(profile, member, context)
        let toolRegistry = try await AgentToolProviderRegistry(
            providers: [chatProvider] + extraProviders
        )
        let model = try await services.makeAgentModel(
            configID: run.modelConfigID,
            policy: policy,
            thinkingLevel: profile.draft.thinkingLevel
        )
        var finalCheckpoint = try await runtime.run(
            checkpoint: checkpoint,
            scope: scope,
            policy: policy,
            model: model,
            tools: toolRegistry.definitions,
            execute: { call in
                if context.lane == .executor {
                    guard let currentDelivery = try await store.delivery(
                        ownerUserID: ownerUserID,
                        deliveryID: delivery.id
                    ), currentDelivery.status == .running,
                    let currentTodo = try await store.todoForDelivery(
                        ownerUserID: ownerUserID,
                        deliveryID: delivery.id
                    ), currentTodo.status == .inProgress,
                    currentTodo.agentID == context.agentID,
                    currentTodo.teamRoomID == context.roomID else {
                        return .failure(
                            "Todo 已被停止或执行身份已经失效；客户端已拒绝本次工具调用。"
                        )
                    }
                }
                return try await toolRegistry.execute(call)
            },
            completionCheck: {
                guard let current = try await store.delivery(
                    ownerUserID: ownerUserID,
                    deliveryID: delivery.id
                ) else { return nil }
                if current.status == .cancelled, context.lane == .executor {
                    return "Todo 已被项目经理停止；执行线程已经结束。"
                }
                guard current.status == .completed else { return nil }
                if current.triggerKind == .todo, current.responseMessageID == nil {
                    return "Todo 已更新并结束本轮执行。"
                }
                if current.lane == .manager, current.responseMessageID == nil {
                    return "Agent 通讯周期已完成。"
                }
                guard let responseMessageID = current.responseMessageID,
                      let response = try await store.message(
                        ownerUserID: ownerUserID,
                        roomID: room.id,
                        messageID: responseMessageID
                      ) else { return nil }
                return response.content
            },
            contextProvider: memoryProvider,
            record: { saved, event in
                try await session.record(checkpoint: saved, event: event)
            }
        )

        let currentDelivery = try await store.delivery(
            ownerUserID: ownerUserID,
            deliveryID: delivery.id
        )
        let executorWasCancelled = context.lane == .executor
            && currentDelivery?.status == .cancelled
        if finalCheckpoint.status == .completed,
           currentDelivery?.status != .completed,
           !executorWasCancelled {
            finalCheckpoint.status = .failed
            finalCheckpoint.stopReason = switch delivery.triggerKind {
            case .todo: "Agent 未通过本地 Relay MCP 更新 Todo 状态并完成本轮执行。"
            default: "Agent 未通过本地 Relay MCP 的 agent_cycle_complete 完成本轮通讯处理。"
            }
        }
        _ = try await session.finish(checkpoint: finalCheckpoint)

        switch finalCheckpoint.status {
        case .completed:
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .completed,
                detail: finalCheckpoint.result
            )
        case .paused, .needsReview, .limitReached:
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .suspended,
                detail: finalCheckpoint.stopReason
            )
        case .ready, .running, .failed:
            let detail = finalCheckpoint.stopReason ?? "本地 Agent 运行未完成。"
            if currentDelivery?.status == .running {
                try await failDeliveryAndNotifyManager(
                    store: store,
                    ownerUserID: ownerUserID,
                    delivery: delivery,
                    runID: context.runID,
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

    private func failDeliveryAndNotifyManager(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        delivery: ProjectAgentDelivery,
        runID: String?,
        detail: String,
        stage: String
    ) async throws {
        let timestamp = now()
        let todo = delivery.triggerKind == .todo
            ? try await store.todoForDelivery(
                ownerUserID: ownerUserID,
                deliveryID: delivery.id
            )
            : nil
        _ = try await store.failDelivery(
            ownerUserID: ownerUserID,
            deliveryID: delivery.id,
            error: detail,
            nowUnixMs: timestamp
        )
        guard let todo else { return }
        var currentTodo = try await store.agentTodo(
            ownerUserID: ownerUserID,
            agentID: delivery.targetAgentID,
            todoID: todo.id
        )
        if currentTodo?.status == .inProgress {
            do {
                currentTodo = try await store.updateAgentTodo(
                    ownerUserID: ownerUserID,
                    agentID: delivery.targetAgentID,
                    todoID: todo.id,
                    update: .init(status: .blocked, blockedReason: detail),
                    nowUnixMs: timestamp
                )
            } catch AgentGroupChatError.conflict {
                // A concurrent manager cancellation is authoritative. Re-read the terminal
                // state below and emit only the idempotent status wake-up it still needs.
                currentTodo = try await store.agentTodo(
                    ownerUserID: ownerUserID,
                    agentID: delivery.targetAgentID,
                    todoID: todo.id
                )
            }
        }
        if todo.status == .inProgress, currentTodo?.status == .blocked {
            _ = try await store.appendAgentTodoProgress(
                ownerUserID: ownerUserID,
                agentID: delivery.targetAgentID,
                todoID: todo.id,
                kind: .blocked,
                runID: runID,
                stage: stage,
                detail: detail,
                nowUnixMs: timestamp
            )
        }
        guard let terminalTodo = currentTodo,
              [.blocked, .completed, .cancelled].contains(terminalTodo.status) else { return }
        _ = try await store.enqueueAgentTodoStatus(
            ownerUserID: ownerUserID,
            agentID: delivery.targetAgentID,
            todoID: todo.id,
            excludingAgentID: nil,
            nowUnixMs: timestamp
        )
    }

    private static func initialMessages(
        profile: LocalAgentProfile,
        member: ProjectAgentRoomMember,
        room: ProjectAgentRoom,
        delivery: ProjectAgentDelivery,
        profession: LocalAgentProfessionDefinition,
        projectType: LocalProjectTypeDefinition?,
        contextLanguage: ChatOSLanguage,
        triggerMessage: ProjectAgentMessage,
        triggerAttachments: [ProjectAgentMessageAttachmentPayload]
    ) -> [AgentMessage] {
        let conversationRole: String
        let conversationContext: String
        switch room.conversationKind {
        case .projectTeam:
            conversationRole = LocalAgentPromptCatalog.render(.conversationRoleProjectTeam)
            conversationContext = LocalAgentPromptCatalog.render(
                .conversationProjectTeam,
                values: [
                    "room_goal": room.draft.goal.isEmpty
                        ? LocalAgentPromptCatalog.render(.roomGoalUnset)
                        : room.draft.goal,
                ]
            )
        case .humanAgentDirect:
            conversationRole = LocalAgentPromptCatalog.render(.conversationRoleHumanAgentDirect)
            conversationContext = LocalAgentPromptCatalog.render(.conversationHumanAgentDirect)
        case .agentAgentDirect:
            conversationRole = LocalAgentPromptCatalog.render(.conversationRoleAgentAgentDirect)
            conversationContext = LocalAgentPromptCatalog.render(.conversationAgentAgentDirect)
        }
        let staffingInstructions = LocalAgentPermission.canManageStaff(
            profile.draft.defaultSkillIDs
        ) ? LocalAgentPromptCatalog.render(.permissionStaffManagement) : ""
        let projectInstructions = LocalAgentPermission.canAccessLocalProjects(
            profile.draft.defaultSkillIDs
        ) ? LocalAgentPromptCatalog.render(.permissionLocalProjects) : ""
        let heartbeatDirective: String
        if delivery.triggerKind == .heartbeat {
            heartbeatDirective = LocalAgentPromptCatalog.render(
                .heartbeatDirective,
                values: [
                    "heartbeat_prompt": profile.draft.heartbeatPrompt.isEmpty
                        ? LocalAgentPromptCatalog.render(.heartbeatDefault)
                        : profile.draft.heartbeatPrompt,
                ]
            )
        } else {
            heartbeatDirective = ""
        }
        let heartbeatInstructions = delivery.lane == .manager
            ? LocalAgentPromptCatalog.render(
                .managerCycle,
                values: ["heartbeat_directive": heartbeatDirective]
            )
            : ""
        let todoInstructions = delivery.triggerKind == .todo
            ? LocalAgentPromptCatalog.render(.executorCycle)
            : ""
        let todoStatusInstructions = delivery.triggerKind == .todoStatus
            ? LocalAgentPromptCatalog.render(.todoStatusCycle)
            : ""
        let professionSkill = LocalAgentPromptCatalog.render(
            .professionSkill,
            values: [
                "skill_name": profession.chatOSSkillName,
                "profession_key": profession.key,
                "skill_markdown": contextLanguage == .english
                    ? profession.skillMarkdownEN
                    : profession.skillMarkdown,
            ]
        )
        let projectSkill: String
        if let projectType {
            projectSkill = LocalAgentPromptCatalog.render(
                .projectSkill,
                values: [
                    "skill_name": projectType.skillName,
                    "project_type_key": projectType.key,
                    "rule_markdown": contextLanguage == .english
                        ? projectType.ruleMarkdownEN
                        : projectType.ruleMarkdown,
                ]
            )
        } else {
            projectSkill = ""
        }
        let system = LocalAgentPromptCatalog.render(
            .groupChatSystem,
            values: [
                "conversation_role": conversationRole,
                "agent_name": profile.draft.name,
                "member_role": member.draft.role,
                "responsibility": member.draft.responsibility.isEmpty
                    ? profile.draft.description
                    : member.draft.responsibility,
                "role_prompt": profile.draft.rolePrompt,
                "conversation_context": conversationContext,
                "capability_discovery_skill": LocalAgentPromptCatalog.render(
                    .capabilityDiscoverySkill
                ),
                "staffing_instructions": staffingInstructions,
                "project_instructions": projectInstructions,
                "manager_instructions": heartbeatInstructions,
                "executor_instructions": todoInstructions,
                "todo_status_instructions": todoStatusInstructions,
                "profession_skill": professionSkill,
                "project_skill": projectSkill,
            ]
        )
        let requestedAction = switch delivery.triggerKind {
        case .heartbeat: LocalAgentPromptCatalog.render(.actionHeartbeat)
        case .todo: LocalAgentPromptCatalog.render(.actionTodo)
        case .todoStatus: LocalAgentPromptCatalog.render(.actionTodoStatus)
        default: LocalAgentPromptCatalog.render(.actionDefault)
        }
        let triggerPayload = (try? JSONEncoder().encode([
            "content": triggerMessage.content,
            "sender_kind": triggerMessage.senderKind.rawValue,
        ])).map { String(decoding: $0, as: UTF8.self) }
            ?? #"{"content":"","sender_kind":"system"}"#
        let envelope = LocalAgentPromptCatalog.render(
            .deliveryUser,
            values: [
                "trigger_kind": delivery.triggerKind.rawValue,
                "attachment_count": String(triggerAttachments.count),
                "trigger_payload": triggerPayload,
                "requested_action": requestedAction,
            ]
        )
        return [
            .init(role: .system, content: system),
            .init(
                role: .user,
                content: envelope,
                attachments: triggerAttachments.map { payload in
                    AgentMessageAttachment(
                        name: payload.attachment.name,
                        mimeType: payload.attachment.mimeType,
                        kind: AgentMessageAttachment.Kind(
                            rawValue: payload.attachment.kind.rawValue
                        ) ?? .file,
                        localFileURL: payload.localFileURL
                    )
                }
            ),
        ]
    }

    private static func failureDetail(_ error: Error) -> String {
        let value = error.localizedDescription.trimmingCharacters(in: .whitespacesAndNewlines)
        return String((value.isEmpty ? "本地 Agent 运行失败。" : value).prefix(8_000))
    }
}
