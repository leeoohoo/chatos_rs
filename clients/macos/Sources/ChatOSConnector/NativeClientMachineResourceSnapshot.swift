import ChatOSNetworking
import ChatOSWorkerRuntime
import Foundation

public enum NativeClientResourceDomain: String, CaseIterable, Sendable, Equatable {
    case swiftAgentRun
    case localAgentHostWorkItem
    case mediaWorker
    case codeReadWorker
    case largeTransfer
    case heavyInProcessMedia
    case pluginProcess
    case databaseMaintenance
}

public enum NativeClientResourceOwner: String, Sendable, Equatable {
    case swiftAgentScheduler
    case rustHostCoordinator
    case fileWorkerGovernor
    case largeTransferGovernor
    case heavyMediaGovernor
    case pluginProcessRegistry
    case databaseMaintenanceOwner
}

public enum NativeClientResourceAdmissionBoundary: String, Sendable, Equatable {
    case beforeDurableClaim
    case dedicatedWorkerBeforeDurableClaim
    case beforeProcessSpawn
    case beforeNetworkIO
    case beforeInProcessDecode
    case processSupervisorOnly
    case quiesceBeforeMaintenance
}

public enum NativeClientDurableClaimStateWhileQueued: String, Sendable, Equatable {
    case unclaimed
    case mayAlreadyBeClaimed
    case notApplicable
}

public enum NativeClientResourceObservationLevel: String, Sendable, Equatable {
    case exact
    case configuredLimitOnly
    case unavailable
}

public enum NativeClientResourceProcessModel: String, Sendable, Equatable {
    case oneWorkerPerUnit
    case hostWorkerPool
    case inProcess
    case supervisedProcessGroup
    case exclusiveOwnerOperation
}

public struct NativeClientResourceDomainSnapshot: Sendable, Equatable {
    public let domain: NativeClientResourceDomain
    public let owner: NativeClientResourceOwner
    public let admissionBoundary: NativeClientResourceAdmissionBoundary
    public let durableClaimStateWhileQueued: NativeClientDurableClaimStateWhileQueued
    public let observationLevel: NativeClientResourceObservationLevel
    public let processModel: NativeClientResourceProcessModel
    public let configuredLimit: Int?
    public let sharedPoolID: String?
    public let sharedPoolLimit: Int?
    public let activeCount: Int?
    public let queuedCount: Int?

    public var permitsUnifiedClaimAdmission: Bool {
        durableClaimStateWhileQueued == .unclaimed
    }
}

public struct NativeClientSharedResourcePoolSnapshot: Sendable, Equatable {
    public let poolID: String
    public let availableResourceUnits: Int
    public let usedResourceUnits: Int
    public let reservedHostResourceUnits: Int
    public let activeCount: Int
    public let queuedCount: Int
}

public struct NativeClientMachineResourceSnapshot: Sendable, Equatable {
    public let domains: [NativeClientResourceDomainSnapshot]
    public let sharedResourcePool: NativeClientSharedResourcePoolSnapshot?

    public func domain(
        _ domain: NativeClientResourceDomain
    ) -> NativeClientResourceDomainSnapshot? {
        domains.first { $0.domain == domain }
    }

    public var domainsMissingExactObservation: [NativeClientResourceDomain] {
        domains.compactMap {
            $0.observationLevel == .exact ? nil : $0.domain
        }
    }

    public var domainsUnsafeForOuterClaimPermit: [NativeClientResourceDomain] {
        domains.compactMap {
            $0.durableClaimStateWhileQueued == .mayAlreadyBeClaimed ? $0.domain : nil
        }
    }
}

/// Produces one machine-level view of every runtime that can materially consume CPU, memory,
/// process, network, or storage capacity. Agent, Media, and Code Read expose their shared
/// claim-before-spawn pool; domains whose work may already own a durable claim remain explicitly
/// excluded so an outer permit cannot make an aging lease wait for capacity.
public struct NativeClientMachineResourceSnapshotProvider: Sendable {
    private let scheduler: LocalAgentGroupChatScheduler
    private let logicalJobBudget: NativeClientResourceBudget
    private let machineResourceGovernor: ClientMachineResourceGovernor
    private let fileWorkerGovernor: ClientFileWorkerResourceGovernor
    private let largeTransferGovernor: ClientLargeTransferGovernor
    private let heavyMediaGovernor: ClientHeavyMediaWorkGovernor
    private let hostSnapshot: @Sendable () async -> NativeLocalAgentHostResourceSnapshot?

    public init(
        scheduler: LocalAgentGroupChatScheduler,
        logicalJobBudget: NativeClientResourceBudget,
        machineResourceGovernor: ClientMachineResourceGovernor = .shared,
        fileWorkerGovernor: ClientFileWorkerResourceGovernor = .shared,
        largeTransferGovernor: ClientLargeTransferGovernor = .shared,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared,
        hostSnapshot: @escaping @Sendable () async -> NativeLocalAgentHostResourceSnapshot? = {
            nil
        }
    ) {
        self.scheduler = scheduler
        self.logicalJobBudget = logicalJobBudget
        self.machineResourceGovernor = machineResourceGovernor
        self.fileWorkerGovernor = fileWorkerGovernor
        self.largeTransferGovernor = largeTransferGovernor
        self.heavyMediaGovernor = heavyMediaGovernor
        self.hostSnapshot = hostSnapshot
    }

    public func snapshot() async -> NativeClientMachineResourceSnapshot {
        async let agent = scheduler.executionGovernorSnapshot()
        async let machine = machineResourceGovernor.snapshot()
        async let files = fileWorkerGovernor.snapshot()
        async let transfers = largeTransferGovernor.snapshot()
        async let heavyMedia = heavyMediaGovernor.snapshot()
        async let host = hostSnapshot()
        let (
            agentSnapshot,
            machineSnapshot,
            fileSnapshot,
            transferSnapshot,
            heavyMediaSnapshot,
            hostSnapshot
        ) = await (
            agent,
            machine,
            files,
            transfers,
            heavyMedia,
            host
        )

        return Self.assemble(
            logicalJobBudget: logicalJobBudget,
            agentSnapshot: agentSnapshot,
            machineSnapshot: machineSnapshot,
            hostSnapshot: hostSnapshot,
            fileSnapshot: fileSnapshot,
            transferSnapshot: transferSnapshot,
            heavyMediaSnapshot: heavyMediaSnapshot
        )
    }

    static func assemble(
        logicalJobBudget: NativeClientResourceBudget,
        agentSnapshot: LocalAgentExecutionGovernorSnapshot,
        machineSnapshot: ClientMachineResourceGovernorSnapshot? = nil,
        hostSnapshot: NativeLocalAgentHostResourceSnapshot? = nil,
        fileSnapshot: ClientFileWorkerGovernorSnapshot,
        transferSnapshot: ClientLargeTransferGovernorSnapshot,
        heavyMediaSnapshot: ClientHeavyMediaWorkGovernorSnapshot
    ) -> NativeClientMachineResourceSnapshot {
        let poolID = "client-machine-resource-units"
        let hostPoolID = "host-reserved-resource-units"
        let sharedPool = machineSnapshot.map {
            NativeClientSharedResourcePoolSnapshot(
                poolID: poolID,
                availableResourceUnits: $0.budget.availableResourceUnits,
                usedResourceUnits: $0.usedResourceUnits,
                reservedHostResourceUnits: $0.budget.reservedHostResourceUnits,
                activeCount: $0.active.count,
                queuedCount: $0.queued.count
            )
        }
        return .init(domains: [
            .init(
                domain: .swiftAgentRun,
                owner: .swiftAgentScheduler,
                admissionBoundary: .beforeDurableClaim,
                durableClaimStateWhileQueued: .unclaimed,
                observationLevel: .exact,
                processModel: .oneWorkerPerUnit,
                configuredLimit: machineSnapshot?.budget.maximumAgentRuns
                    ?? agentSnapshot.limit,
                sharedPoolID: poolID,
                sharedPoolLimit: machineSnapshot?.budget.availableResourceUnits,
                activeCount: Self.count(.agentRun, in: machineSnapshot?.active)
                    ?? agentSnapshot.activeCount,
                queuedCount: Self.count(.agentRun, in: machineSnapshot?.queued)
                    ?? agentSnapshot.queuedCount
            ),
            .init(
                domain: .localAgentHostWorkItem,
                owner: .rustHostCoordinator,
                admissionBoundary: .dedicatedWorkerBeforeDurableClaim,
                durableClaimStateWhileQueued: .unclaimed,
                observationLevel: hostSnapshot == nil ? .configuredLimitOnly : .exact,
                processModel: .hostWorkerPool,
                configuredLimit: hostSnapshot?.maximumConcurrentWorkItems
                    ?? logicalJobBudget.localAgentHostWorkItems,
                sharedPoolID: hostPoolID,
                sharedPoolLimit: machineSnapshot?.budget.reservedHostResourceUnits,
                activeCount: hostSnapshot?.activeWorkItems,
                queuedCount: hostSnapshot?.queuedReadyWorkItems
            ),
            Self.fileDomain(
                .mediaWorker,
                kind: .media,
                snapshot: fileSnapshot,
                machineSnapshot: machineSnapshot,
                poolID: poolID
            ),
            Self.fileDomain(
                .codeReadWorker,
                kind: .codeRead,
                snapshot: fileSnapshot,
                machineSnapshot: machineSnapshot,
                poolID: poolID
            ),
            .init(
                domain: .largeTransfer,
                owner: .largeTransferGovernor,
                admissionBoundary: .beforeNetworkIO,
                durableClaimStateWhileQueued: .mayAlreadyBeClaimed,
                observationLevel: .exact,
                processModel: .inProcess,
                configuredLimit: transferSnapshot.limit,
                sharedPoolID: nil,
                sharedPoolLimit: nil,
                activeCount: transferSnapshot.active.count,
                queuedCount: transferSnapshot.queued.count
            ),
            .init(
                domain: .heavyInProcessMedia,
                owner: .heavyMediaGovernor,
                admissionBoundary: .beforeInProcessDecode,
                durableClaimStateWhileQueued: .mayAlreadyBeClaimed,
                observationLevel: .exact,
                processModel: .inProcess,
                configuredLimit: heavyMediaSnapshot.limit,
                sharedPoolID: nil,
                sharedPoolLimit: nil,
                activeCount: heavyMediaSnapshot.active.count,
                queuedCount: heavyMediaSnapshot.queued.count
            ),
            .init(
                domain: .pluginProcess,
                owner: .pluginProcessRegistry,
                admissionBoundary: .processSupervisorOnly,
                durableClaimStateWhileQueued: .mayAlreadyBeClaimed,
                observationLevel: .unavailable,
                processModel: .supervisedProcessGroup,
                configuredLimit: nil,
                sharedPoolID: nil,
                sharedPoolLimit: nil,
                activeCount: nil,
                queuedCount: nil
            ),
            .init(
                domain: .databaseMaintenance,
                owner: .databaseMaintenanceOwner,
                admissionBoundary: .quiesceBeforeMaintenance,
                durableClaimStateWhileQueued: .notApplicable,
                observationLevel: .unavailable,
                processModel: .exclusiveOwnerOperation,
                configuredLimit: 1,
                sharedPoolID: nil,
                sharedPoolLimit: nil,
                activeCount: nil,
                queuedCount: nil
            ),
        ], sharedResourcePool: sharedPool)
    }

    private static func fileDomain(
        _ domain: NativeClientResourceDomain,
        kind: ClientFileWorkerKind,
        snapshot: ClientFileWorkerGovernorSnapshot,
        machineSnapshot: ClientMachineResourceGovernorSnapshot?,
        poolID: String
    ) -> NativeClientResourceDomainSnapshot {
        let machineKind: ClientMachineResourceKind
        switch kind {
        case .media: machineKind = .mediaWorker
        case .codeRead: machineKind = .codeReadWorker
        }
        return .init(
            domain: domain,
            owner: .fileWorkerGovernor,
            admissionBoundary: .beforeProcessSpawn,
            durableClaimStateWhileQueued: .notApplicable,
            observationLevel: .exact,
            processModel: .oneWorkerPerUnit,
            configuredLimit: snapshot.budget.maximumConcurrentWorkers(for: kind),
            sharedPoolID: poolID,
            sharedPoolLimit: machineSnapshot?.budget.availableResourceUnits,
            activeCount: count(machineKind, in: machineSnapshot?.active)
                ?? snapshot.active.filter { $0.kind == kind }.count,
            queuedCount: count(machineKind, in: machineSnapshot?.queued)
                ?? snapshot.queued.filter { $0.kind == kind }.count
        )
    }

    private static func count(
        _ kind: ClientMachineResourceKind,
        in contexts: [ClientMachineResourceContext]?
    ) -> Int? {
        contexts.map { values in
            values.reduce(into: 0) { total, context in
                if context.kind == kind { total += 1 }
            }
        }
    }
}
