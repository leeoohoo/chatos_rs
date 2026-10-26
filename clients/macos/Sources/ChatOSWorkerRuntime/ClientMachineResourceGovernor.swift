import Foundation

public enum ClientMachineResourceKind: String, CaseIterable, Sendable, Hashable {
    case agentRun
    case mediaWorker
    case codeReadWorker
}

public enum ClientMachineResourcePriority: Int, Sendable, Comparable {
    case background = 0
    case interactive = 1

    public static func < (lhs: Self, rhs: Self) -> Bool { lhs.rawValue < rhs.rawValue }
}

public struct ClientMachineResourceContext: Sendable, Equatable {
    public let requestID: String
    public let kind: ClientMachineResourceKind
    public let priority: ClientMachineResourcePriority
    public let operation: String

    public init(
        requestID: String = UUID().uuidString.lowercased(),
        kind: ClientMachineResourceKind,
        priority: ClientMachineResourcePriority,
        operation: String
    ) {
        self.requestID = requestID
        self.kind = kind
        self.priority = priority
        self.operation = operation
    }
}

public struct ClientMachineResourceBudget: Sendable, Equatable {
    public static let agentRunUnitCost = 2
    public static let fileWorkerUnitCost = 1

    public let availableResourceUnits: Int
    public let reservedHostResourceUnits: Int
    public let maximumAgentRuns: Int
    public let maximumMediaWorkers: Int
    public let maximumCodeReadWorkers: Int

    public init(
        availableResourceUnits: Int,
        reservedHostResourceUnits: Int,
        maximumAgentRuns: Int,
        maximumMediaWorkers: Int,
        maximumCodeReadWorkers: Int
    ) {
        precondition(availableResourceUnits > 0)
        precondition(reservedHostResourceUnits >= 0)
        precondition(maximumAgentRuns >= 0)
        precondition(maximumMediaWorkers >= 0)
        precondition(maximumCodeReadWorkers >= 0)
        precondition(maximumAgentRuns + maximumMediaWorkers + maximumCodeReadWorkers > 0)
        self.availableResourceUnits = availableResourceUnits
        self.reservedHostResourceUnits = reservedHostResourceUnits
        self.maximumAgentRuns = maximumAgentRuns
        self.maximumMediaWorkers = maximumMediaWorkers
        self.maximumCodeReadWorkers = maximumCodeReadWorkers
    }

    public func unitCost(for kind: ClientMachineResourceKind) -> Int {
        kind == .agentRun ? Self.agentRunUnitCost : Self.fileWorkerUnitCost
    }

    public func maximumActive(for kind: ClientMachineResourceKind) -> Int {
        switch kind {
        case .agentRun: maximumAgentRuns
        case .mediaWorker: maximumMediaWorkers
        case .codeReadWorker: maximumCodeReadWorkers
        }
    }

    public static func agentOnly(limit: Int) -> Self {
        .init(
            availableResourceUnits: limit * agentRunUnitCost,
            reservedHostResourceUnits: 0,
            maximumAgentRuns: limit,
            maximumMediaWorkers: 0,
            maximumCodeReadWorkers: 0
        )
    }

    public static func fileOnly(_ budget: ClientFileWorkerResourceBudget) -> Self {
        .init(
            availableResourceUnits: budget.totalConcurrentWorkers,
            reservedHostResourceUnits: 0,
            maximumAgentRuns: 0,
            maximumMediaWorkers: budget.maximumMediaWorkers,
            maximumCodeReadWorkers: budget.maximumCodeReadWorkers
        )
    }
}

public enum ClientMachineResourceBudgetPolicy {
    public static var adaptiveDefault: ClientMachineResourceBudget {
        let logicalProcessorCount = ProcessInfo.processInfo.activeProcessorCount
        let physicalMemoryBytes = ProcessInfo.processInfo.physicalMemory
        let configuredTotal = ProcessInfo.processInfo.environment["CHATOS_BACKGROUND_JOB_BUDGET"]
            .flatMap(Int.init)
            ?? (Bundle.main.object(forInfoDictionaryKey: "ChatOSBackgroundJobBudget")
                as? NSNumber)?.intValue
        let configuredHost = ProcessInfo.processInfo.environment["CHATOS_LOCAL_AGENT_MAX_CONCURRENCY"]
            .flatMap(Int.init)
            ?? (Bundle.main.object(forInfoDictionaryKey: "ChatOSLocalAgentMaxConcurrency")
                as? NSNumber)?.intValue
        return recommended(
            logicalProcessorCount: logicalProcessorCount,
            physicalMemoryBytes: physicalMemoryBytes,
            configuredTotalLogicalJobs: configuredTotal,
            configuredHostWorkItems: configuredHost
        )
    }

    public static func recommended(
        logicalProcessorCount: Int,
        physicalMemoryBytes: UInt64,
        configuredTotalLogicalJobs: Int? = nil,
        configuredHostWorkItems: Int? = nil,
        fileBudget: ClientFileWorkerResourceBudget? = nil
    ) -> ClientMachineResourceBudget {
        let totalJobs = recommendedLogicalJobs(
            logicalProcessorCount: logicalProcessorCount,
            physicalMemoryBytes: physicalMemoryBytes,
            configuredTotalLogicalJobs: configuredTotalLogicalJobs
        )
        let recommendedHost = max(1, totalJobs / 4)
        let requestedHost = configuredHostWorkItems
            .flatMap { (1...16).contains($0) ? $0 : nil } ?? recommendedHost
        let hostWorkItems = min(totalJobs - 1, requestedHost)
        let maximumAgentRuns = totalJobs - hostWorkItems
        let files = fileBudget ?? ClientFileWorkerResourceBudgetPolicy.recommended(
            logicalProcessorCount: logicalProcessorCount,
            physicalMemoryBytes: physicalMemoryBytes
        )
        return .init(
            // One Agent Run is budgeted like two short-lived file workers. Host keeps its hard
            // reservation; Helpers may borrow only the remaining App-owned share.
            availableResourceUnits:
                maximumAgentRuns * ClientMachineResourceBudget.agentRunUnitCost,
            reservedHostResourceUnits:
                hostWorkItems * ClientMachineResourceBudget.agentRunUnitCost,
            maximumAgentRuns: maximumAgentRuns,
            maximumMediaWorkers: files.maximumMediaWorkers,
            maximumCodeReadWorkers: files.maximumCodeReadWorkers
        )
    }

    private static func recommendedLogicalJobs(
        logicalProcessorCount: Int,
        physicalMemoryBytes: UInt64,
        configuredTotalLogicalJobs: Int?
    ) -> Int {
        if let configuredTotalLogicalJobs,
           (2...16).contains(configuredTotalLogicalJobs) {
            return configuredTotalLogicalJobs
        }
        let processorCapacity = max(2, logicalProcessorCount - 2)
        let reservedMemory: UInt64 = 2 * 1_024 * 1_024 * 1_024
        let memoryAvailable = physicalMemoryBytes > reservedMemory
            ? physicalMemoryBytes - reservedMemory
            : 0
        let memoryCapacity = max(
            2,
            Int(min(UInt64(16), memoryAvailable / (2 * 1_024 * 1_024 * 1_024)))
        )
        return max(2, min(16, min(processorCapacity, memoryCapacity)))
    }
}

public struct ClientMachineResourceMetrics: Sendable, Equatable {
    public var admittedCount: UInt64 = 0
    public var queuedAdmissionCount: UInt64 = 0
    public var releasedCount: UInt64 = 0
    public var queuedCancellationCount: UInt64 = 0
    public var queueFullRejectionCount: UInt64 = 0
    public var immediateCapacityRejectionCount: UInt64 = 0
    public var totalQueueWaitNanoseconds: UInt64 = 0
    public var maximumQueueWaitNanoseconds: UInt64 = 0
    public var maximumObservedActive: Int = 0
    public var maximumObservedQueued: Int = 0
    public var interactivePriorityBypassCount: UInt64 = 0
    public var backgroundFairnessAdmissionCount: UInt64 = 0
}

public struct ClientMachineResourceGovernorSnapshot: Sendable, Equatable {
    public let budget: ClientMachineResourceBudget
    public let maximumQueued: Int
    public let active: [ClientMachineResourceContext]
    public let queued: [ClientMachineResourceContext]
    public let metricsByKind: [ClientMachineResourceKind: ClientMachineResourceMetrics]
    public let maximumObservedFileActive: Int
    public let maximumObservedFileQueued: Int
    public let capacity: ClientMachineResourceCapacity
    public let pendingRecoveryState: ClientMachinePressureState?

    public var usedResourceUnits: Int {
        active.reduce(into: 0) { total, context in
            total += budget.unitCost(for: context.kind)
        }
    }

    public func metrics(for kind: ClientMachineResourceKind) -> ClientMachineResourceMetrics {
        metricsByKind[kind] ?? .init()
    }
}

public enum ClientMachineResourceGovernorError: Error, Sendable, Equatable {
    case queueFull
}

public enum ClientMachineMemoryPressure: Int, Sendable, Comparable {
    case normal = 0
    case warning = 1
    case critical = 2

    public static func < (lhs: Self, rhs: Self) -> Bool { lhs.rawValue < rhs.rawValue }
}

public enum ClientMachineThermalPressure: Int, Sendable, Comparable {
    case nominal = 0
    case fair = 1
    case serious = 2
    case critical = 3

    public static func < (lhs: Self, rhs: Self) -> Bool { lhs.rawValue < rhs.rawValue }
}

public enum ClientMachinePressureLevel: Int, Sendable, Comparable {
    case normal = 0
    case elevated = 1
    case serious = 2
    case critical = 3

    public static func < (lhs: Self, rhs: Self) -> Bool { lhs.rawValue < rhs.rawValue }
}

public struct ClientMachinePressureState: Sendable, Equatable {
    public static let normal = Self(
        memory: .normal,
        thermal: .nominal,
        lowPowerModeEnabled: false,
        onBatteryPower: false,
        batteryLevel: nil
    )

    public let memory: ClientMachineMemoryPressure
    public let thermal: ClientMachineThermalPressure
    public let lowPowerModeEnabled: Bool
    public let onBatteryPower: Bool
    public let batteryLevel: Double?

    public init(
        memory: ClientMachineMemoryPressure,
        thermal: ClientMachineThermalPressure,
        lowPowerModeEnabled: Bool,
        onBatteryPower: Bool,
        batteryLevel: Double?
    ) {
        self.memory = memory
        self.thermal = thermal
        self.lowPowerModeEnabled = lowPowerModeEnabled
        self.onBatteryPower = onBatteryPower
        self.batteryLevel = batteryLevel.map { min(1, max(0, $0)) }
    }

    public var isLowBattery: Bool {
        onBatteryPower && batteryLevel.map { $0 <= 0.2 } == true
    }

    public var level: ClientMachinePressureLevel {
        if memory == .critical || thermal == .critical { return .critical }
        if memory == .warning || thermal == .serious { return .serious }
        if thermal == .fair || lowPowerModeEnabled || isLowBattery { return .elevated }
        return .normal
    }
}

public struct ClientMachineResourceCapacity: Sendable, Equatable {
    public let pressureState: ClientMachinePressureState
    public let effectiveResourceUnits: Int
    public let maximumBackgroundResourceUnits: Int
}

public enum ClientMachineResourcePressurePolicy {
    public static let interactiveReserveResourceUnits =
        ClientMachineResourceBudget.agentRunUnitCost

    public static func capacity(
        budget: ClientMachineResourceBudget,
        pressureState: ClientMachinePressureState
    ) -> ClientMachineResourceCapacity {
        let baseline = budget.availableResourceUnits
        let minimum = min(baseline, interactiveReserveResourceUnits)
        let effective: Int
        let background: Int
        switch pressureState.level {
        case .normal:
            effective = baseline
            background = baseline
        case .elevated:
            effective = max(minimum, (baseline * 3 + 3) / 4)
            background = max(0, effective - minimum)
        case .serious:
            effective = max(minimum, (baseline + 1) / 2)
            background = max(0, effective - minimum)
        case .critical:
            effective = minimum
            background = 0
        }
        return .init(
            pressureState: pressureState,
            effectiveResourceUnits: effective,
            maximumBackgroundResourceUnits: background
        )
    }
}

public final class ClientMachineResourcePermit: @unchecked Sendable {
    public let context: ClientMachineResourceContext

    private let lock = NSLock()
    private let id: UUID
    private var governor: ClientMachineResourceGovernor?

    fileprivate init(
        id: UUID,
        context: ClientMachineResourceContext,
        governor: ClientMachineResourceGovernor
    ) {
        self.id = id
        self.context = context
        self.governor = governor
    }

    public func release() {
        guard let governor = takeGovernor() else { return }
        Task { await governor.release(id: id) }
    }

    public func releaseAndWait() async {
        guard let governor = takeGovernor() else { return }
        await governor.release(id: id)
    }

    deinit { release() }

    private func takeGovernor() -> ClientMachineResourceGovernor? {
        lock.lock()
        let governor = governor
        self.governor = nil
        lock.unlock()
        return governor
    }
}

/// One claim-before-spawn admission queue shared by Agent Runs, Media Workers, and Code Read
/// Workers. Host workers retain a separately reserved budget because their durable claims are
/// owned in the Rust process; the reservation is visible here but never represented by a fake
/// in-process permit.
public actor ClientMachineResourceGovernor {
    public static let shared = ClientMachineResourceGovernor(
        budget: ClientMachineResourceBudgetPolicy.adaptiveDefault
    )

    private struct Waiter {
        let id: UUID
        let context: ClientMachineResourceContext
        let enqueuedAtUptimeNanoseconds: UInt64
        let continuation: CheckedContinuation<ClientMachineResourcePermit?, Never>
    }

    private static let maximumInteractiveAdmissionBurst = 4
    public let budget: ClientMachineResourceBudget
    public let maximumQueued: Int
    private var active: [UUID: ClientMachineResourceContext] = [:]
    private var waiters: [Waiter] = []
    private var metricsByKind: [ClientMachineResourceKind: ClientMachineResourceMetrics] = [:]
    private var consecutiveInteractiveAdmissionsWhileContended = 0
    private var maximumObservedFileActive = 0
    private var maximumObservedFileQueued = 0
    private var capacity: ClientMachineResourceCapacity
    private var pendingRecoveryState: ClientMachinePressureState?
    private var recoveryTask: Task<Void, Never>?
    private var recoveryGeneration: UInt64 = 0
    private let recoveryDelay: Duration

    public init(
        budget: ClientMachineResourceBudget,
        maximumQueued: Int = 256,
        recoveryDelay: Duration = .seconds(30)
    ) {
        precondition(maximumQueued > 0)
        precondition(recoveryDelay >= .zero)
        self.budget = budget
        self.maximumQueued = maximumQueued
        self.recoveryDelay = recoveryDelay
        self.capacity = ClientMachineResourcePressurePolicy.capacity(
            budget: budget,
            pressureState: .normal
        )
    }

    public func acquire(
        context: ClientMachineResourceContext
    ) async throws -> ClientMachineResourcePermit {
        try Task.checkCancellation()
        let id = UUID()
        if canAdmit(context) {
            return admit(id: id, context: context, wasQueued: false)
        }
        guard waiters.count < maximumQueued else {
            updateMetrics(for: context.kind) { Self.increment(&$0.queueFullRejectionCount) }
            throw ClientMachineResourceGovernorError.queueFull
        }
        let permit = await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                waiters.append(.init(
                    id: id,
                    context: context,
                    enqueuedAtUptimeNanoseconds: DispatchTime.now().uptimeNanoseconds,
                    continuation: continuation
                ))
                updateMetrics(for: context.kind) {
                    $0.maximumObservedQueued = max($0.maximumObservedQueued, queuedCount(context.kind))
                }
                maximumObservedFileQueued = max(
                    maximumObservedFileQueued,
                    fileQueuedCount()
                )
            }
        } onCancel: {
            Task { await self.cancel(id: id) }
        }
        guard let permit else { throw CancellationError() }
        if Task.isCancelled {
            await permit.releaseAndWait()
            throw CancellationError()
        }
        return permit
    }

    public func tryAcquire(
        context: ClientMachineResourceContext
    ) -> ClientMachineResourcePermit? {
        guard !Task.isCancelled, canAdmit(context) else {
            updateMetrics(for: context.kind) {
                Self.increment(&$0.immediateCapacityRejectionCount)
            }
            return nil
        }
        return admit(id: UUID(), context: context, wasQueued: false)
    }

    public func snapshot() -> ClientMachineResourceGovernorSnapshot {
        .init(
            budget: budget,
            maximumQueued: maximumQueued,
            active: active.values.sorted(by: Self.contextOrder),
            queued: waiters.map(\.context),
            metricsByKind: metricsByKind,
            maximumObservedFileActive: maximumObservedFileActive,
            maximumObservedFileQueued: maximumObservedFileQueued,
            capacity: capacity,
            pendingRecoveryState: pendingRecoveryState
        )
    }

    /// Applies resource pressure without revoking permits that already crossed the admission
    /// boundary. Escalations take effect immediately; recovery is delayed so oscillating system
    /// notifications cannot repeatedly open and close process-spawn capacity.
    public func updatePressureState(_ pressureState: ClientMachinePressureState) {
        if pressureState.level >= capacity.pressureState.level {
            cancelPendingRecovery()
            capacity = ClientMachineResourcePressurePolicy.capacity(
                budget: budget,
                pressureState: pressureState
            )
            drainWaiters()
            return
        }

        scheduleRecovery(to: pressureState)
    }

    fileprivate func release(id: UUID) {
        guard let context = active.removeValue(forKey: id) else { return }
        updateMetrics(for: context.kind) { Self.increment(&$0.releasedCount) }
        drainWaiters()
    }

    private func cancel(id: UUID) {
        guard let index = waiters.firstIndex(where: { $0.id == id }) else { return }
        let waiter = waiters.remove(at: index)
        updateMetrics(for: waiter.context.kind) {
            Self.increment(&$0.queuedCancellationCount)
        }
        waiter.continuation.resume(returning: nil)
    }

    private func drainWaiters() {
        while let index = nextAdmissibleWaiterIndex() {
            let waiter = waiters.remove(at: index)
            let permit = admit(
                id: waiter.id,
                context: waiter.context,
                wasQueued: true,
                enqueuedAtUptimeNanoseconds: waiter.enqueuedAtUptimeNanoseconds
            )
            waiter.continuation.resume(returning: permit)
        }
    }

    private func nextAdmissibleWaiterIndex() -> Int? {
        let candidates = waiters.indices.filter { canAdmit(waiters[$0].context) }
        guard !candidates.isEmpty else { return nil }
        let interactive = candidates.first { waiters[$0].context.priority == .interactive }
        let background = candidates.first { waiters[$0].context.priority == .background }
        if let background,
           interactive == nil
            || consecutiveInteractiveAdmissionsWhileContended
                >= Self.maximumInteractiveAdmissionBurst {
            consecutiveInteractiveAdmissionsWhileContended = 0
            if interactive != nil {
                updateMetrics(for: waiters[background].context.kind) {
                    Self.increment(&$0.backgroundFairnessAdmissionCount)
                }
            }
            return background
        }
        if let interactive {
            if let background {
                consecutiveInteractiveAdmissionsWhileContended += 1
                if background < interactive {
                    updateMetrics(for: waiters[interactive].context.kind) {
                        Self.increment(&$0.interactivePriorityBypassCount)
                    }
                }
            } else {
                consecutiveInteractiveAdmissionsWhileContended = 0
            }
            return interactive
        }
        consecutiveInteractiveAdmissionsWhileContended = 0
        return background
    }

    private func canAdmit(_ context: ClientMachineResourceContext) -> Bool {
        let kind = context.kind
        let cost = budget.unitCost(for: kind)
        guard budget.maximumActive(for: kind) > activeCount(kind),
              usedResourceUnits() + cost <= capacity.effectiveResourceUnits else { return false }
        if context.priority == .background,
           usedBackgroundResourceUnits() + cost
            > capacity.maximumBackgroundResourceUnits {
            return false
        }
        // Preserve the historical one-slot reservation between the two file-worker kinds. Across
        // Agent and file work the pool stays work-conserving; once another domain queues, normal
        // priority/fairness ordering prevents the releasing domain from immediately taking it back.
        guard kind != .agentRun else { return true }
        let otherFileKind: ClientMachineResourceKind = kind == .mediaWorker
            ? .codeReadWorker
            : .mediaWorker
        // Once a file kind is already using the pool, keep one unit available for the other
        // kind. The first file request is still allowed to borrow the final spare unit beside an
        // Agent Run; otherwise both file kinds would reject that unit while each waited for the
        // other to occupy it.
        let reserve = activeCount(kind) > 0 && activeCount(otherFileKind) == 0
            ? budget.unitCost(for: otherFileKind)
            : 0
        return usedResourceUnits() + cost
            <= capacity.effectiveResourceUnits - reserve
    }

    private func admit(
        id: UUID,
        context: ClientMachineResourceContext,
        wasQueued: Bool,
        enqueuedAtUptimeNanoseconds: UInt64? = nil
    ) -> ClientMachineResourcePermit {
        active[id] = context
        maximumObservedFileActive = max(maximumObservedFileActive, fileActiveCount())
        let wait = enqueuedAtUptimeNanoseconds.map {
            DispatchTime.now().uptimeNanoseconds - $0
        } ?? 0
        updateMetrics(for: context.kind) {
            Self.increment(&$0.admittedCount)
            if wasQueued {
                Self.increment(&$0.queuedAdmissionCount)
                Self.add(wait, to: &$0.totalQueueWaitNanoseconds)
                $0.maximumQueueWaitNanoseconds = max($0.maximumQueueWaitNanoseconds, wait)
            }
            $0.maximumObservedActive = max($0.maximumObservedActive, activeCount(context.kind))
        }
        return .init(id: id, context: context, governor: self)
    }

    private func activeCount(_ kind: ClientMachineResourceKind) -> Int {
        active.values.reduce(into: 0) { count, context in
            if context.kind == kind { count += 1 }
        }
    }

    private func queuedCount(_ kind: ClientMachineResourceKind) -> Int {
        waiters.reduce(into: 0) { count, waiter in
            if waiter.context.kind == kind { count += 1 }
        }
    }

    private func usedResourceUnits() -> Int {
        active.values.reduce(into: 0) { total, context in
            total += budget.unitCost(for: context.kind)
        }
    }

    private func usedBackgroundResourceUnits() -> Int {
        active.values.reduce(into: 0) { total, context in
            if context.priority == .background {
                total += budget.unitCost(for: context.kind)
            }
        }
    }

    private func scheduleRecovery(to pressureState: ClientMachinePressureState) {
        recoveryGeneration &+= 1
        let generation = recoveryGeneration
        recoveryTask?.cancel()
        pendingRecoveryState = pressureState
        let delay = recoveryDelay
        recoveryTask = Task { [weak self] in
            do {
                try await Task.sleep(for: delay)
            } catch {
                return
            }
            guard !Task.isCancelled else { return }
            await self?.completeRecovery(
                to: pressureState,
                generation: generation
            )
        }
    }

    private func completeRecovery(
        to pressureState: ClientMachinePressureState,
        generation: UInt64
    ) {
        guard generation == recoveryGeneration,
              pendingRecoveryState == pressureState else { return }
        capacity = ClientMachineResourcePressurePolicy.capacity(
            budget: budget,
            pressureState: pressureState
        )
        pendingRecoveryState = nil
        recoveryTask = nil
        drainWaiters()
    }

    private func cancelPendingRecovery() {
        recoveryGeneration &+= 1
        recoveryTask?.cancel()
        recoveryTask = nil
        pendingRecoveryState = nil
    }

    private func fileActiveCount() -> Int {
        active.values.reduce(into: 0) { count, context in
            if context.kind != .agentRun { count += 1 }
        }
    }

    private func fileQueuedCount() -> Int {
        waiters.reduce(into: 0) { count, waiter in
            if waiter.context.kind != .agentRun { count += 1 }
        }
    }

    private func updateMetrics(
        for kind: ClientMachineResourceKind,
        _ update: (inout ClientMachineResourceMetrics) -> Void
    ) {
        var metrics = metricsByKind[kind] ?? .init()
        update(&metrics)
        metricsByKind[kind] = metrics
    }

    private static func contextOrder(
        _ lhs: ClientMachineResourceContext,
        _ rhs: ClientMachineResourceContext
    ) -> Bool {
        if lhs.kind != rhs.kind { return lhs.kind.rawValue < rhs.kind.rawValue }
        return lhs.requestID < rhs.requestID
    }

    private static func increment(_ value: inout UInt64) {
        if value < .max { value += 1 }
    }

    private static func add(_ amount: UInt64, to value: inout UInt64) {
        let result = value.addingReportingOverflow(amount)
        value = result.overflow ? .max : result.partialValue
    }
}
