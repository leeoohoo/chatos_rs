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

enum LocalAgentExecutionPriority: Sendable, Equatable {
    case interactive
    case background
}

public struct LocalAgentExecutionGovernorMetrics: Sendable, Equatable {
    public let admittedCount: UInt64
    public let queuedAdmissionCount: UInt64
    public let releasedCount: UInt64
    public let queuedCancellationCount: UInt64
    public let queueFullRejectionCount: UInt64
    public let immediateCapacityRejectionCount: UInt64
    public let totalQueueWaitNanoseconds: UInt64
    public let maximumQueueWaitNanoseconds: UInt64
    public let maximumObservedActive: Int
    public let maximumObservedQueued: Int
    public let interactivePriorityBypassCount: UInt64
    public let backgroundFairnessAdmissionCount: UInt64
}

public struct LocalAgentExecutionGovernorSnapshot: Sendable, Equatable {
    public let limit: Int
    public let maximumQueued: Int
    public let activeCount: Int
    public let queuedCount: Int
    public let interactiveQueuedCount: Int
    public let backgroundQueuedCount: Int
    public let totalInFlight: Int
    public let metrics: LocalAgentExecutionGovernorMetrics
}

/// Bounds execution across account, room, communication and recovery drains. A permit is
/// acquired before a durable delivery is claimed, so work never waits for capacity while its
/// database lease is already aging.
actor LocalAgentExecutionGovernor {
    private struct Waiter {
        let id: UUID
        let priority: LocalAgentExecutionPriority
        let enqueuedAtUptimeNanoseconds: UInt64
        let continuation: CheckedContinuation<LocalAgentExecutionPermit?, Never>
    }

    private struct Metrics {
        var admittedCount: UInt64 = 0
        var queuedAdmissionCount: UInt64 = 0
        var releasedCount: UInt64 = 0
        var queuedCancellationCount: UInt64 = 0
        var queueFullRejectionCount: UInt64 = 0
        var immediateCapacityRejectionCount: UInt64 = 0
        var totalQueueWaitNanoseconds: UInt64 = 0
        var maximumQueueWaitNanoseconds: UInt64 = 0
        var maximumObservedActive: Int = 0
        var maximumObservedQueued: Int = 0
        var interactivePriorityBypassCount: UInt64 = 0
        var backgroundFairnessAdmissionCount: UInt64 = 0
    }

    private static let maximumInteractiveAdmissionBurst = 4
    let limit: Int
    let maximumQueued: Int
    private var active = 0
    private var waiters: [Waiter] = []
    private var consecutiveInteractiveAdmissionsWhileContended = 0
    private var metrics = Metrics()

    init(limit: Int, maximumQueued: Int = 256) {
        precondition(limit > 0)
        precondition(maximumQueued > 0)
        self.limit = limit
        self.maximumQueued = maximumQueued
    }

    func acquire(
        priority: LocalAgentExecutionPriority = .background
    ) async -> LocalAgentExecutionPermit? {
        guard !Task.isCancelled else { return nil }
        if active < limit {
            active += 1
            recordAdmission(wasQueued: false, queueWaitNanoseconds: 0)
            return .init(governor: self)
        }
        guard waiters.count < maximumQueued else {
            Self.increment(&metrics.queueFullRejectionCount)
            return nil
        }
        let waiterID = UUID()
        let permit = await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                waiters.append(.init(
                    id: waiterID,
                    priority: priority,
                    enqueuedAtUptimeNanoseconds: DispatchTime.now().uptimeNanoseconds,
                    continuation: continuation
                ))
                metrics.maximumObservedQueued = max(
                    metrics.maximumObservedQueued,
                    waiters.count
                )
            }
        } onCancel: {
            Task { await self.cancel(waiterID: waiterID) }
        }
        if Task.isCancelled {
            await permit?.releaseAndWait()
            return nil
        }
        return permit
    }

    func tryAcquire() -> LocalAgentExecutionPermit? {
        guard active < limit else {
            Self.increment(&metrics.immediateCapacityRejectionCount)
            return nil
        }
        active += 1
        recordAdmission(wasQueued: false, queueWaitNanoseconds: 0)
        return .init(governor: self)
    }

    func snapshot() -> LocalAgentExecutionGovernorSnapshot {
        let interactiveQueuedCount = waiters.reduce(into: 0) { total, waiter in
            if waiter.priority == .interactive { total += 1 }
        }
        return .init(
            limit: limit,
            maximumQueued: maximumQueued,
            activeCount: active,
            queuedCount: waiters.count,
            interactiveQueuedCount: interactiveQueuedCount,
            backgroundQueuedCount: waiters.count - interactiveQueuedCount,
            totalInFlight: active + waiters.count,
            metrics: .init(
                admittedCount: metrics.admittedCount,
                queuedAdmissionCount: metrics.queuedAdmissionCount,
                releasedCount: metrics.releasedCount,
                queuedCancellationCount: metrics.queuedCancellationCount,
                queueFullRejectionCount: metrics.queueFullRejectionCount,
                immediateCapacityRejectionCount: metrics.immediateCapacityRejectionCount,
                totalQueueWaitNanoseconds: metrics.totalQueueWaitNanoseconds,
                maximumQueueWaitNanoseconds: metrics.maximumQueueWaitNanoseconds,
                maximumObservedActive: metrics.maximumObservedActive,
                maximumObservedQueued: metrics.maximumObservedQueued,
                interactivePriorityBypassCount: metrics.interactivePriorityBypassCount,
                backgroundFairnessAdmissionCount: metrics.backgroundFairnessAdmissionCount
            )
        )
    }

    fileprivate func release() {
        precondition(active > 0)
        Self.increment(&metrics.releasedCount)
        while !waiters.isEmpty {
            let waiter = waiters.remove(at: nextWaiterIndex())
            let queueWaitNanoseconds = DispatchTime.now().uptimeNanoseconds
                - waiter.enqueuedAtUptimeNanoseconds
            recordAdmission(wasQueued: true, queueWaitNanoseconds: queueWaitNanoseconds)
            waiter.continuation.resume(returning: .init(governor: self))
            return
        }
        active -= 1
        consecutiveInteractiveAdmissionsWhileContended = 0
    }

    private func nextWaiterIndex() -> Int {
        let interactiveIndex = waiters.firstIndex { $0.priority == .interactive }
        let backgroundIndex = waiters.firstIndex { $0.priority == .background }
        switch (interactiveIndex, backgroundIndex) {
        case let (.some(interactiveIndex), .some(backgroundIndex)):
            if consecutiveInteractiveAdmissionsWhileContended
                < Self.maximumInteractiveAdmissionBurst {
                consecutiveInteractiveAdmissionsWhileContended += 1
                if backgroundIndex < interactiveIndex {
                    Self.increment(&metrics.interactivePriorityBypassCount)
                }
                return interactiveIndex
            }
            consecutiveInteractiveAdmissionsWhileContended = 0
            Self.increment(&metrics.backgroundFairnessAdmissionCount)
            return backgroundIndex
        case let (.some(interactiveIndex), .none):
            consecutiveInteractiveAdmissionsWhileContended = 0
            return interactiveIndex
        case let (.none, .some(backgroundIndex)):
            consecutiveInteractiveAdmissionsWhileContended = 0
            return backgroundIndex
        case (.none, .none):
            preconditionFailure("execution governor selected from an empty queue")
        }
    }

    private func cancel(waiterID: UUID) {
        guard let index = waiters.firstIndex(where: { $0.id == waiterID }) else { return }
        let waiter = waiters.remove(at: index)
        Self.increment(&metrics.queuedCancellationCount)
        waiter.continuation.resume(returning: nil)
    }

    private func recordAdmission(
        wasQueued: Bool,
        queueWaitNanoseconds: UInt64
    ) {
        Self.increment(&metrics.admittedCount)
        metrics.maximumObservedActive = max(metrics.maximumObservedActive, active)
        guard wasQueued else { return }
        Self.increment(&metrics.queuedAdmissionCount)
        Self.add(queueWaitNanoseconds, to: &metrics.totalQueueWaitNanoseconds)
        metrics.maximumQueueWaitNanoseconds = max(
            metrics.maximumQueueWaitNanoseconds,
            queueWaitNanoseconds
        )
    }

    private static func increment(_ value: inout UInt64) {
        guard value < .max else { return }
        value += 1
    }

    private static func add(_ amount: UInt64, to value: inout UInt64) {
        let result = value.addingReportingOverflow(amount)
        value = result.overflow ? .max : result.partialValue
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
    let runtime: any AgentRuntimeExecuting
    let limits: AgentGroupChatRoutingLimits
    let relayMCP: LocalAgentRelayMCPServer
    let executorTaskRegistry: LocalAgentExecutorTaskRegistry
    let activeDeliveryRegistry: LocalAgentActiveDeliveryRegistry
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
        runtime: any AgentRuntimeExecuting = AgentRuntime(),
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

    public func executionGovernorSnapshot() async -> LocalAgentExecutionGovernorSnapshot {
        await executionGovernor.snapshot()
    }

}
