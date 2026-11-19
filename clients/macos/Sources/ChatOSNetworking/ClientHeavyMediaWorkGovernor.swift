import Foundation

public enum ClientHeavyMediaWorkKind: String, Sendable, Equatable {
    case imageDecode
    case videoFrameExtraction
}

public struct ClientHeavyMediaWorkContext: Sendable, Equatable {
    public let taskID: String
    public let ownerSessionID: String?
    public let kind: ClientHeavyMediaWorkKind
    public let estimatedDecodedBytes: Int

    public init(
        taskID: String,
        ownerSessionID: String? = nil,
        kind: ClientHeavyMediaWorkKind,
        estimatedDecodedBytes: Int
    ) {
        self.taskID = taskID
        self.ownerSessionID = ownerSessionID
        self.kind = kind
        self.estimatedDecodedBytes = estimatedDecodedBytes
    }
}

public struct ClientHeavyMediaWorkGovernorSnapshot: Sendable, Equatable {
    public let limit: Int
    public let maximumEstimatedDecodedBytes: Int
    public let active: [ClientHeavyMediaWorkContext]
    public let queued: [ClientHeavyMediaWorkContext]
    public let totalInFlight: Int
    public let metrics: ClientHeavyMediaWorkGovernorMetrics
}

public struct ClientHeavyMediaWorkGovernorMetrics: Sendable, Equatable {
    public let admittedCount: UInt64
    public let queuedAdmissionCount: UInt64
    public let releasedCount: UInt64
    public let queuedCancellationCount: UInt64
    public let queueFullRejectionCount: UInt64
    public let invalidEstimateRejectionCount: UInt64
    public let overBudgetRejectionCount: UInt64
    public let totalQueueWaitNanoseconds: UInt64
    public let maximumQueueWaitNanoseconds: UInt64
    public let maximumObservedActive: Int
    public let maximumObservedQueued: Int
}

public enum ClientHeavyMediaWorkGovernorError: Error, Sendable, Equatable {
    case queueFull
    case invalidEstimatedDecodedBytes
    case estimatedDecodedBytesExceedLimit(maximumBytes: Int)
}

public final class ClientHeavyMediaWorkPermit: @unchecked Sendable {
    public let context: ClientHeavyMediaWorkContext

    private let lock = NSLock()
    private let id: UUID
    private var governor: ClientHeavyMediaWorkGovernor?

    fileprivate init(
        id: UUID,
        context: ClientHeavyMediaWorkContext,
        governor: ClientHeavyMediaWorkGovernor
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

    private func takeGovernor() -> ClientHeavyMediaWorkGovernor? {
        lock.lock()
        let governor = governor
        self.governor = nil
        lock.unlock()
        return governor
    }
}

/// Process-wide admission for remaining in-process image normalization paths.
/// Process-isolated media workers deliberately bypass this actor so unrelated jobs can run
/// concurrently behind separate PID, timeout, cancellation, and failure boundaries.
public actor ClientHeavyMediaWorkGovernor {
    public static let shared = ClientHeavyMediaWorkGovernor(limit: 1)

    public let limit: Int
    public let maximumQueued: Int
    public let maximumEstimatedDecodedBytes: Int
    private let admissions: ClientFIFOAdmissionQueue<ClientHeavyMediaWorkContext>
    private var invalidEstimateRejectionCount: UInt64 = 0
    private var overBudgetRejectionCount: UInt64 = 0

    public init(
        limit: Int = 1,
        maximumQueued: Int = 256,
        maximumEstimatedDecodedBytes: Int = 256 * 1_024 * 1_024
    ) {
        precondition(limit > 0)
        precondition(maximumQueued > 0)
        precondition(maximumEstimatedDecodedBytes > 0)
        self.limit = limit
        self.maximumQueued = maximumQueued
        self.maximumEstimatedDecodedBytes = maximumEstimatedDecodedBytes
        self.admissions = .init(limit: limit, maximumQueued: maximumQueued)
    }

    public func acquire(
        context: ClientHeavyMediaWorkContext
    ) async throws -> ClientHeavyMediaWorkPermit {
        try Task.checkCancellation()
        guard context.estimatedDecodedBytes > 0 else {
            Self.increment(&invalidEstimateRejectionCount)
            throw ClientHeavyMediaWorkGovernorError.invalidEstimatedDecodedBytes
        }
        guard context.estimatedDecodedBytes <= maximumEstimatedDecodedBytes else {
            Self.increment(&overBudgetRejectionCount)
            throw ClientHeavyMediaWorkGovernorError.estimatedDecodedBytesExceedLimit(
                maximumBytes: maximumEstimatedDecodedBytes
            )
        }
        do {
            let id = try await admissions.acquire(context: context)
            return .init(id: id, context: context, governor: self)
        } catch ClientFIFOAdmissionQueueError.queueFull {
            throw ClientHeavyMediaWorkGovernorError.queueFull
        }
    }

    public nonisolated func perform<Value: Sendable>(
        context: ClientHeavyMediaWorkContext,
        operation: @escaping @Sendable () async throws -> Value
    ) async throws -> Value {
        let permit = try await acquire(context: context)
        do {
            let value = try await operation()
            await permit.releaseAndWait()
            return value
        } catch {
            await permit.releaseAndWait()
            throw error
        }
    }

    public func snapshot() async -> ClientHeavyMediaWorkGovernorSnapshot {
        let snapshot = await admissions.snapshot()
        let active = snapshot.active.sorted(by: Self.contextOrder)
        return .init(
            limit: limit,
            maximumEstimatedDecodedBytes: maximumEstimatedDecodedBytes,
            active: active,
            queued: snapshot.queued,
            totalInFlight: active.count + snapshot.queued.count,
            metrics: .init(
                admittedCount: snapshot.metrics.admittedCount,
                queuedAdmissionCount: snapshot.metrics.queuedAdmissionCount,
                releasedCount: snapshot.metrics.releasedCount,
                queuedCancellationCount: snapshot.metrics.queuedCancellationCount,
                queueFullRejectionCount: snapshot.metrics.queueFullRejectionCount,
                invalidEstimateRejectionCount: invalidEstimateRejectionCount,
                overBudgetRejectionCount: overBudgetRejectionCount,
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

    private static func increment(_ value: inout UInt64) {
        guard value < .max else { return }
        value += 1
    }

    private static func contextOrder(
        _ lhs: ClientHeavyMediaWorkContext,
        _ rhs: ClientHeavyMediaWorkContext
    ) -> Bool {
        if lhs.taskID != rhs.taskID { return lhs.taskID < rhs.taskID }
        return lhs.kind.rawValue < rhs.kind.rawValue
    }
}
