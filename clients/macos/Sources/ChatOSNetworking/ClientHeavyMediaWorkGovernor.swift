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

/// Process-wide admission for memory-intensive image normalization and video frame extraction.
/// This actor limits transient decode working sets; it does not own media jobs or their leases.
public actor ClientHeavyMediaWorkGovernor {
    public static let shared = ClientHeavyMediaWorkGovernor(limit: 1)

    private struct Waiter {
        let id: UUID
        let context: ClientHeavyMediaWorkContext
        let continuation: CheckedContinuation<ClientHeavyMediaWorkPermit?, Never>
    }

    public let limit: Int
    public let maximumQueued: Int
    public let maximumEstimatedDecodedBytes: Int
    private var active: [UUID: ClientHeavyMediaWorkContext] = [:]
    private var waiters: [Waiter] = []

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
    }

    public func acquire(
        context: ClientHeavyMediaWorkContext
    ) async throws -> ClientHeavyMediaWorkPermit {
        try Task.checkCancellation()
        guard context.estimatedDecodedBytes > 0 else {
            throw ClientHeavyMediaWorkGovernorError.invalidEstimatedDecodedBytes
        }
        guard context.estimatedDecodedBytes <= maximumEstimatedDecodedBytes else {
            throw ClientHeavyMediaWorkGovernorError.estimatedDecodedBytesExceedLimit(
                maximumBytes: maximumEstimatedDecodedBytes
            )
        }
        let id = UUID()
        if active.count < limit {
            active[id] = context
            return .init(id: id, context: context, governor: self)
        }
        guard waiters.count < maximumQueued else {
            throw ClientHeavyMediaWorkGovernorError.queueFull
        }
        let permit = await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                waiters.append(.init(
                    id: id,
                    context: context,
                    continuation: continuation
                ))
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

    public func snapshot() -> ClientHeavyMediaWorkGovernorSnapshot {
        .init(
            limit: limit,
            maximumEstimatedDecodedBytes: maximumEstimatedDecodedBytes,
            active: active.values.sorted(by: Self.contextOrder),
            queued: waiters.map(\.context)
        )
    }

    fileprivate func release(id: UUID) {
        guard active.removeValue(forKey: id) != nil else { return }
        guard !waiters.isEmpty else { return }
        let waiter = waiters.removeFirst()
        active[waiter.id] = waiter.context
        waiter.continuation.resume(returning: .init(
            id: waiter.id,
            context: waiter.context,
            governor: self
        ))
    }

    private func cancel(id: UUID) {
        guard let index = waiters.firstIndex(where: { $0.id == id }) else { return }
        let waiter = waiters.remove(at: index)
        waiter.continuation.resume(returning: nil)
    }

    private static func contextOrder(
        _ lhs: ClientHeavyMediaWorkContext,
        _ rhs: ClientHeavyMediaWorkContext
    ) -> Bool {
        if lhs.taskID != rhs.taskID { return lhs.taskID < rhs.taskID }
        return lhs.kind.rawValue < rhs.kind.rawValue
    }
}
