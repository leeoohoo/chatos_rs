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
    private var machinePermit: ClientMachineResourcePermit?

    fileprivate init(
        context: ClientFileWorkerContext,
        machinePermit: ClientMachineResourcePermit
    ) {
        self.context = context
        self.machinePermit = machinePermit
    }

    public func release() {
        takeMachinePermit()?.release()
    }

    public func releaseAndWait() async {
        await takeMachinePermit()?.releaseAndWait()
    }

    deinit { release() }

    private func takeMachinePermit() -> ClientMachineResourcePermit? {
        lock.lock()
        let permit = machinePermit
        machinePermit = nil
        lock.unlock()
        return permit
    }
}

/// Bounds short-lived Media and Code Read helpers without merging their work into one serial
/// lane. Each kind owns one hard reservation, so a burst from one kind cannot consume the last
/// slot needed by the other; remaining slots are shared for throughput.
public actor ClientFileWorkerResourceGovernor {
    public static let shared = ClientFileWorkerResourceGovernor(
        budget: ClientFileWorkerResourceBudgetPolicy.adaptiveDefault,
        machineGovernor: .shared
    )
    public let budget: ClientFileWorkerResourceBudget
    public let maximumQueued: Int
    private let machineGovernor: ClientMachineResourceGovernor

    public init(
        budget: ClientFileWorkerResourceBudget =
            ClientFileWorkerResourceBudgetPolicy.adaptiveDefault,
        maximumQueued: Int = 256,
        machineGovernor: ClientMachineResourceGovernor? = nil
    ) {
        precondition(maximumQueued > 0)
        self.budget = budget
        self.maximumQueued = maximumQueued
        self.machineGovernor = machineGovernor ?? .init(
            budget: .fileOnly(budget),
            maximumQueued: maximumQueued
        )
    }

    public func acquire(
        context: ClientFileWorkerContext
    ) async throws -> ClientFileWorkerPermit {
        try Task.checkCancellation()
        do {
            let permit = try await machineGovernor.acquire(context: .init(
                requestID: context.requestID,
                kind: context.kind.machineKind,
                priority: context.priority.machinePriority,
                operation: context.operation
            ))
            return .init(context: context, machinePermit: permit)
        } catch ClientMachineResourceGovernorError.queueFull {
            throw ClientFileWorkerGovernorError.queueFull
        }
    }

    public func snapshot() async -> ClientFileWorkerGovernorSnapshot {
        let machine = await machineGovernor.snapshot()
        let active = machine.active.compactMap(ClientFileWorkerContext.init(machine:))
        let queued = machine.queued.compactMap(ClientFileWorkerContext.init(machine:))
        let media = machine.metrics(for: .mediaWorker)
        let codeRead = machine.metrics(for: .codeReadWorker)
        return .init(
            budget: budget,
            active: active,
            queued: queued,
            metrics: .init(
                admittedCount: Self.add(media.admittedCount, codeRead.admittedCount),
                queuedAdmissionCount: Self.add(
                    media.queuedAdmissionCount,
                    codeRead.queuedAdmissionCount
                ),
                releasedCount: Self.add(media.releasedCount, codeRead.releasedCount),
                queuedCancellationCount: Self.add(
                    media.queuedCancellationCount,
                    codeRead.queuedCancellationCount
                ),
                queueFullRejectionCount: Self.add(
                    media.queueFullRejectionCount,
                    codeRead.queueFullRejectionCount
                ),
                maximumObservedActive: machine.maximumObservedFileActive,
                maximumObservedQueued: machine.maximumObservedFileQueued
            )
        )
    }

    private static func add(_ lhs: UInt64, _ rhs: UInt64) -> UInt64 {
        let result = lhs.addingReportingOverflow(rhs)
        return result.overflow ? .max : result.partialValue
    }
}

private extension ClientFileWorkerKind {
    var machineKind: ClientMachineResourceKind {
        switch self {
        case .media: .mediaWorker
        case .codeRead: .codeReadWorker
        }
    }
}

private extension ClientFileWorkerPriority {
    var machinePriority: ClientMachineResourcePriority {
        switch self {
        case .background: .background
        case .interactive: .interactive
        }
    }
}

private extension ClientFileWorkerContext {
    init?(machine: ClientMachineResourceContext) {
        let kind: ClientFileWorkerKind
        switch machine.kind {
        case .agentRun: return nil
        case .mediaWorker: kind = .media
        case .codeReadWorker: kind = .codeRead
        }
        self.init(
            requestID: machine.requestID,
            kind: kind,
            priority: machine.priority == .interactive ? .interactive : .background,
            operation: machine.operation
        )
    }
}
