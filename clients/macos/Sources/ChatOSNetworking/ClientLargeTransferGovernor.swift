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

    public let limit: Int
    public let maximumQueued: Int
    private let admissions: ClientFIFOAdmissionQueue<ClientLargeTransferContext>

    public init(limit: Int = 2, maximumQueued: Int = 256) {
        precondition(limit > 0)
        precondition(maximumQueued > 0)
        self.limit = limit
        self.maximumQueued = maximumQueued
        self.admissions = .init(limit: limit, maximumQueued: maximumQueued)
    }

    public func acquire(
        context: ClientLargeTransferContext
    ) async throws -> ClientLargeTransferPermit {
        do {
            let id = try await admissions.acquire(context: context)
            return .init(id: id, context: context, governor: self)
        } catch ClientFIFOAdmissionQueueError.queueFull {
            throw ClientLargeTransferGovernorError.queueFull
        }
    }

    public func snapshot() async -> ClientLargeTransferGovernorSnapshot {
        let snapshot = await admissions.snapshot()
        let active = snapshot.active.sorted(by: Self.contextOrder)
        return .init(
            limit: limit,
            active: active,
            queued: snapshot.queued,
            totalInFlight: active.count + snapshot.queued.count,
            metrics: .init(
                admittedCount: snapshot.metrics.admittedCount,
                queuedAdmissionCount: snapshot.metrics.queuedAdmissionCount,
                releasedCount: snapshot.metrics.releasedCount,
                queuedCancellationCount: snapshot.metrics.queuedCancellationCount,
                queueFullRejectionCount: snapshot.metrics.queueFullRejectionCount,
                totalQueueWaitNanoseconds: snapshot.metrics.totalQueueWaitNanoseconds,
                maximumQueueWaitNanoseconds: snapshot.metrics.maximumQueueWaitNanoseconds,
                maximumObservedActive: snapshot.metrics.maximumObservedActive,
                maximumObservedQueued: snapshot.metrics.maximumObservedQueued
            )
        )
    }

    fileprivate func release(id: UUID) async {
        await admissions.release(id: id)
    }

    private static func contextOrder(
        _ lhs: ClientLargeTransferContext,
        _ rhs: ClientLargeTransferContext
    ) -> Bool {
        if lhs.taskID != rhs.taskID { return lhs.taskID < rhs.taskID }
        return lhs.direction.rawValue < rhs.direction.rawValue
    }
}
