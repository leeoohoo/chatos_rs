import Foundation

enum ClientFIFOAdmissionQueueError: Error {
    case queueFull
}

struct ClientFIFOAdmissionQueueMetrics: Sendable {
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

struct ClientFIFOAdmissionQueueSnapshot<Context: Sendable>: Sendable {
    let active: [Context]
    let queued: [Context]
    let metrics: ClientFIFOAdmissionQueueMetrics
}

actor ClientFIFOAdmissionQueue<Context: Sendable> {
    private struct Waiter {
        let id: UUID
        let context: Context
        let enqueuedAtUptimeNanoseconds: UInt64
        let continuation: CheckedContinuation<Bool, Never>
    }

    private let limit: Int
    private let maximumQueued: Int
    private var active: [UUID: Context] = [:]
    private var waiters: [Waiter] = []
    private var metrics = ClientFIFOAdmissionQueueMetrics()

    init(limit: Int, maximumQueued: Int) {
        self.limit = limit
        self.maximumQueued = maximumQueued
    }

    func acquire(context: Context) async throws -> UUID {
        try Task.checkCancellation()
        let id = UUID()
        if active.count < limit {
            active[id] = context
            recordAdmission(wasQueued: false, queueWaitNanoseconds: 0)
            return id
        }
        guard waiters.count < maximumQueued else {
            Self.increment(&metrics.queueFullRejectionCount)
            throw ClientFIFOAdmissionQueueError.queueFull
        }
        let admitted = await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                waiters.append(.init(
                    id: id,
                    context: context,
                    enqueuedAtUptimeNanoseconds: DispatchTime.now().uptimeNanoseconds,
                    continuation: continuation
                ))
                metrics.maximumObservedQueued = max(metrics.maximumObservedQueued, waiters.count)
            }
        } onCancel: {
            Task { await self.cancel(id: id) }
        }
        guard admitted else { throw CancellationError() }
        if Task.isCancelled {
            release(id: id)
            throw CancellationError()
        }
        return id
    }

    func release(id: UUID) {
        guard active.removeValue(forKey: id) != nil else { return }
        Self.increment(&metrics.releasedCount)
        guard !waiters.isEmpty else { return }
        let waiter = waiters.removeFirst()
        active[waiter.id] = waiter.context
        let wait = DispatchTime.now().uptimeNanoseconds - waiter.enqueuedAtUptimeNanoseconds
        recordAdmission(wasQueued: true, queueWaitNanoseconds: wait)
        waiter.continuation.resume(returning: true)
    }

    func snapshot() -> ClientFIFOAdmissionQueueSnapshot<Context> {
        .init(
            active: Array(active.values),
            queued: waiters.map(\.context),
            metrics: metrics
        )
    }

    private func cancel(id: UUID) {
        guard let index = waiters.firstIndex(where: { $0.id == id }) else { return }
        let waiter = waiters.remove(at: index)
        Self.increment(&metrics.queuedCancellationCount)
        waiter.continuation.resume(returning: false)
    }

    private func recordAdmission(wasQueued: Bool, queueWaitNanoseconds: UInt64) {
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
}
