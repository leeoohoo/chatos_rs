import ChatOSAgentRuntime
import ChatOSCore
import Foundation

final class LocalAgentExecutorCancellationHandle: @unchecked Sendable {
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

actor LocalAgentExecutorTaskRegistry {
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

/// Distinguishes a live delivery owned by this scheduler from a durable checkpoint left behind by
/// an interrupted app process. Recovery may run alongside the communication fast lane, so the
/// database's `running` state alone is not enough to decide whether a checkpoint needs resuming.
actor LocalAgentActiveDeliveryRegistry {
    private var deliveryIDs: Set<String> = []

    /// Atomically claims in-process ownership of a delivery. Manual resume, Human retry, and
    /// crash recovery all use this gate so a durable `running` checkpoint cannot be executed by
    /// two tasks at once.
    func acquire(deliveryID: String) -> Bool {
        deliveryIDs.insert(deliveryID).inserted
    }

    func unregister(deliveryID: String) {
        deliveryIDs.remove(deliveryID)
    }
}

/// Serializes account-wide drain passes created by multiple windows and the heartbeat loop. The
/// durable SQLite queue remains authoritative; this lease only prevents two local consumers from
/// observing the same Agent work queue as busy and leaving newly queued work stranded between passes.
actor LocalAgentAccountDrainCoordinator {
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

final class LocalAgentExecutionPermit: @unchecked Sendable {
    private let lock = NSLock()
    private var governor: LocalAgentExecutionGovernor?

    init(governor: LocalAgentExecutionGovernor) {
        self.governor = governor
    }

    private func takeGovernor() -> LocalAgentExecutionGovernor? {
        lock.lock()
        let governor = governor
        self.governor = nil
        lock.unlock()
        return governor
    }

    func release() {
        guard let governor = takeGovernor() else { return }
        Task { await governor.release() }
    }

    func releaseAndWait() async {
        guard let governor = takeGovernor() else { return }
        await governor.release()
    }

    deinit {
        release()
    }
}

/// Bounds execution across account, room, communication and recovery drains. A permit is
/// acquired before a durable delivery is claimed, so work never waits for capacity while its
/// database lease is already aging.
actor LocalAgentExecutionGovernor {
    private struct Waiter {
        let id: UUID
        let continuation: CheckedContinuation<LocalAgentExecutionPermit?, Never>
    }

    let limit: Int
    private var active = 0
    private var waiters: [Waiter] = []

    init(limit: Int) {
        precondition(limit > 0)
        self.limit = limit
    }

    func acquire() async -> LocalAgentExecutionPermit? {
        guard !Task.isCancelled else { return nil }
        if active < limit {
            active += 1
            return .init(governor: self)
        }
        let waiterID = UUID()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                waiters.append(.init(id: waiterID, continuation: continuation))
            }
        } onCancel: {
            Task { await self.cancel(waiterID: waiterID) }
        }
    }

    func tryAcquire() -> LocalAgentExecutionPermit? {
        guard active < limit else { return nil }
        active += 1
        return .init(governor: self)
    }

    fileprivate func release() {
        while !waiters.isEmpty {
            let waiter = waiters.removeFirst()
            waiter.continuation.resume(returning: .init(governor: self))
            return
        }
        precondition(active > 0)
        active -= 1
    }

    private func cancel(waiterID: UUID) {
        guard let index = waiters.firstIndex(where: { $0.id == waiterID }) else { return }
        let waiter = waiters.remove(at: index)
        waiter.continuation.resume(returning: nil)
    }
}

/// Consumes the durable room delivery queue entirely in the macOS client. The server is used
/// only through `AgentServiceProviding` for account-owned model credentials and Memory Engine;
/// it never selects an Agent, routes a message, or owns a run checkpoint.
public struct LocalAgentGroupChatScheduler: Sendable {
    public enum DeliveryAttemptOutcome: String, Sendable, Equatable {
        case completed
        case suspended
        case failed
    }

    /// Ephemeral receipt returned to the scheduler caller after one durable delivery attempt.
    /// Agent context, Memory and conversation history live elsewhere; this is not model input.
    public struct DeliveryAttemptReceipt: Sendable, Equatable {
        public let deliveryID: String
        public let agentID: String
        public let outcome: DeliveryAttemptOutcome
        public let detail: String?

        public init(
            deliveryID: String,
            agentID: String,
            outcome: DeliveryAttemptOutcome,
            detail: String?
        ) {
            self.deliveryID = deliveryID
            self.agentID = agentID
            self.outcome = outcome
            self.detail = detail
        }
    }

    struct ClaimedWork: Sendable {
        let order: Int
        let room: ProjectAgentRoom
        let member: ProjectAgentRoomMember
        let delivery: ProjectAgentDelivery
        let executionPermit: LocalAgentExecutionPermit
    }

    struct OrderedDeliveryAttemptReceipt: Sendable {
        let order: Int
        let receipt: DeliveryAttemptReceipt
    }

    public typealias AdditionalToolProviderFactory = @Sendable (
        _ profile: LocalAgentProfile,
        _ member: ProjectAgentRoomMember,
        _ context: LocalAgentChatRunContext,
        _ productSkillSession: ProductToolSkillSession
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

    let service: NativeAgentGroupChatService
    let services: any AgentServiceProviding
    let settings: AgentSettingsStore
    let runtime: AgentRuntime
    let limits: AgentGroupChatRoutingLimits
    let relayMCP: LocalAgentRelayMCPServer
    let executorTaskRegistry: LocalAgentExecutorTaskRegistry
    let activeDeliveryRegistry: LocalAgentActiveDeliveryRegistry
    let accountDrainCoordinator: LocalAgentAccountDrainCoordinator
    let executionGovernor: LocalAgentExecutionGovernor
    let additionalToolProviders: AdditionalToolProviderFactory
    let projectTypeKeyProvider: ProjectTypeKeyProvider
    let professionProvider: ProfessionProvider
    let projectTypeProvider: ProjectTypeProvider
    let professionCatalogProvider: ProfessionCatalogProvider
    let todoPluginCatalogProvider: TodoPluginCatalogProvider
    let contextLanguageProvider: ContextLanguageProvider
    let now: @Sendable () -> Int64

    public init(
        service: NativeAgentGroupChatService,
        services: any AgentServiceProviding,
        settings: AgentSettingsStore = .init(),
        runtime: AgentRuntime = .init(),
        limits: AgentGroupChatRoutingLimits = .init(),
        maximumConcurrentExecutions: Int = NativeClientResourceBudgetPolicy.adaptiveDefault.swiftAgentRuns,
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
        additionalToolProviders: @escaping AdditionalToolProviderFactory = { _, _, _, _ in [] },
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
        self.activeDeliveryRegistry = LocalAgentActiveDeliveryRegistry()
        self.accountDrainCoordinator = LocalAgentAccountDrainCoordinator()
        self.executionGovernor = LocalAgentExecutionGovernor(
            limit: max(1, maximumConcurrentExecutions)
        )
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

}
