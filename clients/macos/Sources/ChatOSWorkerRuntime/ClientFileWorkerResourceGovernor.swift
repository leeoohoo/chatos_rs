import Foundation

public enum ClientFileWorkerKind: String, CaseIterable, Sendable, Hashable {
    case media
    case codeRead
}

public enum ClientFileWorkerPriority: Int, Sendable, Comparable {
    case background = 0
    case interactive = 1

    public static func < (lhs: Self, rhs: Self) -> Bool { lhs.rawValue < rhs.rawValue }
}

public struct ClientFileWorkerContext: Sendable, Equatable {
    public let requestID: String
    public let kind: ClientFileWorkerKind
    public let priority: ClientFileWorkerPriority
    public let operation: String

    public init(
        requestID: String = UUID().uuidString.lowercased(),
        kind: ClientFileWorkerKind,
        priority: ClientFileWorkerPriority,
        operation: String
    ) {
        self.requestID = requestID
        self.kind = kind
        self.priority = priority
        self.operation = operation
    }
}

public struct ClientFileWorkerResourceBudget: Sendable, Equatable {
    public let totalConcurrentWorkers: Int
    public let maximumMediaWorkers: Int
    public let maximumCodeReadWorkers: Int

    public init(
        totalConcurrentWorkers: Int,
        maximumMediaWorkers: Int,
        maximumCodeReadWorkers: Int
    ) {
        precondition(totalConcurrentWorkers >= ClientFileWorkerKind.allCases.count + 1)
        precondition((2...totalConcurrentWorkers).contains(maximumMediaWorkers))
        precondition((2...totalConcurrentWorkers).contains(maximumCodeReadWorkers))
        self.totalConcurrentWorkers = totalConcurrentWorkers
        self.maximumMediaWorkers = maximumMediaWorkers
        self.maximumCodeReadWorkers = maximumCodeReadWorkers
    }

    public func maximumConcurrentWorkers(for kind: ClientFileWorkerKind) -> Int {
        switch kind {
        case .media: maximumMediaWorkers
        case .codeRead: maximumCodeReadWorkers
        }
    }
}

public enum ClientFileWorkerResourceBudgetPolicy {
    public static let minimumConcurrentWorkers = 3
    public static let maximumConcurrentWorkers = 10
    public static let reservedApplicationMemoryBytes: UInt64 = 2 * 1_024 * 1_024 * 1_024
    public static let memoryBytesPerWorker: UInt64 = 1 * 1_024 * 1_024 * 1_024

    public static var adaptiveDefault: ClientFileWorkerResourceBudget {
        recommended(
            logicalProcessorCount: ProcessInfo.processInfo.activeProcessorCount,
            physicalMemoryBytes: ProcessInfo.processInfo.physicalMemory
        )
    }

    public static func recommended(
        logicalProcessorCount: Int,
        physicalMemoryBytes: UInt64,
        configuredTotalConcurrentWorkers: Int? = nil
    ) -> ClientFileWorkerResourceBudget {
        let processorCapacity = max(minimumConcurrentWorkers, logicalProcessorCount - 2)
        let availableMemory = physicalMemoryBytes > reservedApplicationMemoryBytes
            ? physicalMemoryBytes - reservedApplicationMemoryBytes
            : 0
        let memoryCapacity = max(
            minimumConcurrentWorkers,
            Int(min(
                UInt64(maximumConcurrentWorkers),
                availableMemory / memoryBytesPerWorker
            ))
        )
        let adaptiveTotal = max(
            minimumConcurrentWorkers,
            min(maximumConcurrentWorkers, min(processorCapacity, memoryCapacity))
        )
        let total = configuredTotalConcurrentWorkers.flatMap(validatedTotal) ?? adaptiveTotal
        return .init(
            totalConcurrentWorkers: total,
            maximumMediaWorkers: min(4, total - 1),
            maximumCodeReadWorkers: min(8, total - 1)
        )
    }

    public static func validatedTotal(_ value: Int) -> Int? {
        (minimumConcurrentWorkers...maximumConcurrentWorkers).contains(value) ? value : nil
    }
}

public struct ClientFileWorkerGovernorMetrics: Sendable, Equatable {
    public let admittedCount: UInt64
    public let queuedAdmissionCount: UInt64
    public let releasedCount: UInt64
    public let queuedCancellationCount: UInt64
    public let queueFullRejectionCount: UInt64
    public let maximumObservedActive: Int
    public let maximumObservedQueued: Int
}

public struct ClientFileWorkerGovernorSnapshot: Sendable, Equatable {
    public let budget: ClientFileWorkerResourceBudget
    public let active: [ClientFileWorkerContext]
    public let queued: [ClientFileWorkerContext]
    public let metrics: ClientFileWorkerGovernorMetrics
}

public enum ClientFileWorkerGovernorError: Error, Sendable, Equatable {
    case queueFull
}

public final class ClientFileWorkerPermit: @unchecked Sendable {
    public let context: ClientFileWorkerContext

    private let lock = NSLock()
    private let id: UUID
    private var governor: ClientFileWorkerResourceGovernor?

    fileprivate init(
        id: UUID,
        context: ClientFileWorkerContext,
        governor: ClientFileWorkerResourceGovernor
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

    private func takeGovernor() -> ClientFileWorkerResourceGovernor? {
        lock.lock()
        let governor = governor
        self.governor = nil
        lock.unlock()
        return governor
    }
}

/// Bounds short-lived Media and Code Read helpers without merging their work into one serial
/// lane. Each kind owns one hard reservation, so a burst from one kind cannot consume the last
/// slot needed by the other; remaining slots are shared for throughput.
public actor ClientFileWorkerResourceGovernor {
    public static let shared = ClientFileWorkerResourceGovernor(
        budget: ClientFileWorkerResourceBudgetPolicy.adaptiveDefault
    )

    private struct Waiter {
        let id: UUID
        let context: ClientFileWorkerContext
        let continuation: CheckedContinuation<ClientFileWorkerPermit?, Never>
    }

    private struct Metrics {
        var admittedCount: UInt64 = 0
        var queuedAdmissionCount: UInt64 = 0
        var releasedCount: UInt64 = 0
        var queuedCancellationCount: UInt64 = 0
        var queueFullRejectionCount: UInt64 = 0
        var maximumObservedActive: Int = 0
        var maximumObservedQueued: Int = 0
    }

    private static let maximumInteractiveAdmissionBurst = 4
    public let budget: ClientFileWorkerResourceBudget
    public let maximumQueued: Int
    private var active: [UUID: ClientFileWorkerContext] = [:]
    private var waiters: [Waiter] = []
    private var consecutiveInteractiveAdmissionsWhileContended = 0
    private var metrics = Metrics()

    public init(
        budget: ClientFileWorkerResourceBudget =
            ClientFileWorkerResourceBudgetPolicy.adaptiveDefault,
        maximumQueued: Int = 256
    ) {
        precondition(maximumQueued > 0)
        self.budget = budget
        self.maximumQueued = maximumQueued
    }

    public func acquire(
        context: ClientFileWorkerContext
    ) async throws -> ClientFileWorkerPermit {
        try Task.checkCancellation()
        let id = UUID()
        if canAdmit(context.kind) {
            return admit(id: id, context: context, wasQueued: false)
        }
        guard waiters.count < maximumQueued else {
            Self.increment(&metrics.queueFullRejectionCount)
            throw ClientFileWorkerGovernorError.queueFull
        }
        let permit = await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                waiters.append(.init(id: id, context: context, continuation: continuation))
                metrics.maximumObservedQueued = max(metrics.maximumObservedQueued, waiters.count)
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

    public func snapshot() -> ClientFileWorkerGovernorSnapshot {
        .init(
            budget: budget,
            active: active.values.sorted(by: Self.contextOrder),
            queued: waiters.map(\.context),
            metrics: .init(
                admittedCount: metrics.admittedCount,
                queuedAdmissionCount: metrics.queuedAdmissionCount,
                releasedCount: metrics.releasedCount,
                queuedCancellationCount: metrics.queuedCancellationCount,
                queueFullRejectionCount: metrics.queueFullRejectionCount,
                maximumObservedActive: metrics.maximumObservedActive,
                maximumObservedQueued: metrics.maximumObservedQueued
            )
        )
    }

    fileprivate func release(id: UUID) {
        guard active.removeValue(forKey: id) != nil else { return }
        Self.increment(&metrics.releasedCount)
        drainWaiters()
    }

    private func cancel(id: UUID) {
        guard let index = waiters.firstIndex(where: { $0.id == id }) else { return }
        let waiter = waiters.remove(at: index)
        Self.increment(&metrics.queuedCancellationCount)
        waiter.continuation.resume(returning: nil)
    }

    private func drainWaiters() {
        while let index = nextAdmissibleWaiterIndex() {
            let waiter = waiters.remove(at: index)
            let permit = admit(id: waiter.id, context: waiter.context, wasQueued: true)
            waiter.continuation.resume(returning: permit)
        }
    }

    private func nextAdmissibleWaiterIndex() -> Int? {
        let candidates = waiters.indices.filter { canAdmit(waiters[$0].context.kind) }
        guard !candidates.isEmpty else { return nil }
        let interactive = candidates.first {
            waiters[$0].context.priority == .interactive
        }
        let background = candidates.first {
            waiters[$0].context.priority == .background
        }
        if let background,
           interactive == nil
            || consecutiveInteractiveAdmissionsWhileContended
                >= Self.maximumInteractiveAdmissionBurst {
            consecutiveInteractiveAdmissionsWhileContended = 0
            return background
        }
        if let interactive {
            if background != nil {
                consecutiveInteractiveAdmissionsWhileContended += 1
            } else {
                consecutiveInteractiveAdmissionsWhileContended = 0
            }
            return interactive
        }
        consecutiveInteractiveAdmissionsWhileContended = 0
        return background
    }

    private func canAdmit(_ kind: ClientFileWorkerKind) -> Bool {
        let activeForKind = active.values.reduce(into: 0) { count, context in
            if context.kind == kind { count += 1 }
        }
        guard activeForKind < budget.maximumConcurrentWorkers(for: kind) else { return false }
        let reservationsForOtherKinds = ClientFileWorkerKind.allCases.reduce(into: 0) {
            reserved, candidate in
            guard candidate != kind else { return }
            if !active.values.contains(where: { $0.kind == candidate }) { reserved += 1 }
        }
        return active.count < budget.totalConcurrentWorkers - reservationsForOtherKinds
    }

    private func admit(
        id: UUID,
        context: ClientFileWorkerContext,
        wasQueued: Bool
    ) -> ClientFileWorkerPermit {
        active[id] = context
        Self.increment(&metrics.admittedCount)
        if wasQueued { Self.increment(&metrics.queuedAdmissionCount) }
        metrics.maximumObservedActive = max(metrics.maximumObservedActive, active.count)
        return .init(id: id, context: context, governor: self)
    }

    private static func contextOrder(
        _ lhs: ClientFileWorkerContext,
        _ rhs: ClientFileWorkerContext
    ) -> Bool {
        if lhs.kind != rhs.kind { return lhs.kind.rawValue < rhs.kind.rawValue }
        return lhs.requestID < rhs.requestID
    }

    private static func increment(_ value: inout UInt64) {
        if value < UInt64.max { value += 1 }
    }
}
