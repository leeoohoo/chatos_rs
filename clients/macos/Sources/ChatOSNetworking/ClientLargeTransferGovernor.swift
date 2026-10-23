import Foundation

public enum ClientLargeTransferDirection: String, Sendable, Equatable {
    case upload
    case download
}

public struct ClientLargeTransferContext: Sendable, Equatable {
    public let taskID: String
    public let ownerSessionID: String?
    public let direction: ClientLargeTransferDirection

    public init(
        taskID: String,
        ownerSessionID: String? = nil,
        direction: ClientLargeTransferDirection
    ) {
        self.taskID = taskID
        self.ownerSessionID = ownerSessionID
        self.direction = direction
    }
}
public struct ClientLargeTransferGovernorSnapshot: Sendable, Equatable {
    public let limit: Int
    public let active: [ClientLargeTransferContext]
    public let queued: [ClientLargeTransferContext]
    public let totalInFlight: Int
    public let metrics: ClientLargeTransferGovernorMetrics
}

public struct ClientLargeTransferGovernorMetrics: Sendable, Equatable {
    public let admittedCount: UInt64
    public let queuedAdmissionCount: UInt64
    public let releasedCount: UInt64
    public let queuedCancellationCount: UInt64
    public let queueFullRejectionCount: UInt64
    public let totalQueueWaitNanoseconds: UInt64
    public let maximumQueueWaitNanoseconds: UInt64
    public let maximumObservedActive: Int
    public let maximumObservedQueued: Int
}

public enum ClientLargeTransferGovernorError: Error, Sendable, Equatable {
    case queueFull
}

public final class ClientLargeTransferPermit: @unchecked Sendable {
    public let context: ClientLargeTransferContext

    private let lock = NSLock()
    private let id: UUID
    private var governor: ClientLargeTransferGovernor?

    fileprivate init(
        id: UUID,
        context: ClientLargeTransferContext,
        governor: ClientLargeTransferGovernor
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

    private func takeGovernor() -> ClientLargeTransferGovernor? {
        lock.lock()
        let governor = governor
        self.governor = nil
        lock.unlock()
        return governor
    }
}

/// Process-wide admission for file-sized network transfers. The governor does not claim or own
/// durable jobs; callers acquire before starting network I/O and keep their existing task/job
/// identity authoritative. FIFO admission and cancellation prevent a burst from multiplying the
/// independent Media and Agent transport limits.
public actor ClientLargeTransferGovernor {
    public static let shared = ClientLargeTransferGovernor(limit: 2)

    private struct Waiter {
        let id: UUID
        let context: ClientLargeTransferContext
        let enqueuedAtUptimeNanoseconds: UInt64
        let continuation: CheckedContinuation<ClientLargeTransferPermit?, Never>
    }

    private struct Metrics {
        var admittedCount: UInt64 = 0
        var queuedAdmissionCount: UInt64 = 0
        var releasedCount: UInt64 = 0
        var queuedCancellationCount: UInt64 = 0
        var queueFullRejectionCount: UInt64 = 0
        var totalQueueWaitNanoseconds: UInt64 = 0
        var maximumQueueWaitNanoseconds: UInt64 = 0
        var maximumObservedActive: Int = 0
        var maximumObservedQueued: Int = 0
    }

    public let limit: Int
    public let maximumQueued: Int
    private var active: [UUID: ClientLargeTransferContext] = [:]
    private var waiters: [Waiter] = []
    private var metrics = Metrics()

    public init(limit: Int = 2, maximumQueued: Int = 256) {
        precondition(limit > 0)
        precondition(maximumQueued > 0)
        self.limit = limit
        self.maximumQueued = maximumQueued
    }

    public func acquire(
        context: ClientLargeTransferContext
    ) async throws -> ClientLargeTransferPermit {
        try Task.checkCancellation()
        let id = UUID()
        if active.count < limit {
            active[id] = context
            recordAdmission(wasQueued: false, queueWaitNanoseconds: 0)
            return .init(id: id, context: context, governor: self)
        }
        guard waiters.count < maximumQueued else {
            Self.increment(&metrics.queueFullRejectionCount)
            throw ClientLargeTransferGovernorError.queueFull
        }
        let permit = await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                waiters.append(.init(
                    id: id,
                    context: context,
                    enqueuedAtUptimeNanoseconds: DispatchTime.now().uptimeNanoseconds,
                    continuation: continuation
                ))
                metrics.maximumObservedQueued = max(
                    metrics.maximumObservedQueued,
                    waiters.count
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

    public func snapshot() -> ClientLargeTransferGovernorSnapshot {
        .init(
            limit: limit,
            active: active.values.sorted(by: Self.contextOrder),
            queued: waiters.map(\.context),
            totalInFlight: active.count + waiters.count,
            metrics: .init(
                admittedCount: metrics.admittedCount,
                queuedAdmissionCount: metrics.queuedAdmissionCount,
                releasedCount: metrics.releasedCount,
                queuedCancellationCount: metrics.queuedCancellationCount,
                queueFullRejectionCount: metrics.queueFullRejectionCount,
                totalQueueWaitNanoseconds: metrics.totalQueueWaitNanoseconds,
                maximumQueueWaitNanoseconds: metrics.maximumQueueWaitNanoseconds,
                maximumObservedActive: metrics.maximumObservedActive,
                maximumObservedQueued: metrics.maximumObservedQueued
            )
        )
    }

    fileprivate func release(id: UUID) {
        guard active.removeValue(forKey: id) != nil else { return }
        Self.increment(&metrics.releasedCount)
        guard !waiters.isEmpty else { return }
        let waiter = waiters.removeFirst()
        active[waiter.id] = waiter.context
        let queueWaitNanoseconds = DispatchTime.now().uptimeNanoseconds
            - waiter.enqueuedAtUptimeNanoseconds
        recordAdmission(wasQueued: true, queueWaitNanoseconds: queueWaitNanoseconds)
        waiter.continuation.resume(returning: .init(
            id: waiter.id,
            context: waiter.context,
            governor: self
        ))
    }

    private func cancel(id: UUID) {
        guard let index = waiters.firstIndex(where: { $0.id == id }) else { return }
        let waiter = waiters.remove(at: index)
        Self.increment(&metrics.queuedCancellationCount)
        waiter.continuation.resume(returning: nil)
    }

    private func recordAdmission(
        wasQueued: Bool,
        queueWaitNanoseconds: UInt64
    ) {
        Self.increment(&metrics.admittedCount)
        metrics.maximumObservedActive = max(metrics.maximumObservedActive, active.count)
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

    private static func contextOrder(
        _ lhs: ClientLargeTransferContext,
        _ rhs: ClientLargeTransferContext
    ) -> Bool {
        if lhs.taskID != rhs.taskID { return lhs.taskID < rhs.taskID }
        return lhs.direction.rawValue < rhs.direction.rawValue
    }
}
