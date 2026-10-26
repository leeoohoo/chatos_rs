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

public struct NativeClientMachineResourceSnapshot: Sendable, Equatable {
    public let domains: [NativeClientResourceDomainSnapshot]

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
/// process, network, or storage capacity. This is deliberately observation-only: domains whose
/// work may already own a durable claim are marked as such so a future shared governor cannot be
/// bolted around them and make an aging lease wait for capacity.
public struct NativeClientMachineResourceSnapshotProvider: Sendable {
    private let scheduler: LocalAgentGroupChatScheduler
    private let logicalJobBudget: NativeClientResourceBudget
    private let fileWorkerGovernor: ClientFileWorkerResourceGovernor
    private let largeTransferGovernor: ClientLargeTransferGovernor
    private let heavyMediaGovernor: ClientHeavyMediaWorkGovernor

    public init(
        scheduler: LocalAgentGroupChatScheduler,
        logicalJobBudget: NativeClientResourceBudget,
        fileWorkerGovernor: ClientFileWorkerResourceGovernor = .shared,
        largeTransferGovernor: ClientLargeTransferGovernor = .shared,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared
    ) {
        self.scheduler = scheduler
        self.logicalJobBudget = logicalJobBudget
        self.fileWorkerGovernor = fileWorkerGovernor
        self.largeTransferGovernor = largeTransferGovernor
        self.heavyMediaGovernor = heavyMediaGovernor
    }

    public func snapshot() async -> NativeClientMachineResourceSnapshot {
        async let agent = scheduler.executionGovernorSnapshot()
        async let files = fileWorkerGovernor.snapshot()
        async let transfers = largeTransferGovernor.snapshot()
        async let heavyMedia = heavyMediaGovernor.snapshot()
        let (agentSnapshot, fileSnapshot, transferSnapshot, heavyMediaSnapshot) = await (
            agent,
            files,
            transfers,
            heavyMedia
        )

        return Self.assemble(
            logicalJobBudget: logicalJobBudget,
            agentSnapshot: agentSnapshot,
            fileSnapshot: fileSnapshot,
            transferSnapshot: transferSnapshot,
            heavyMediaSnapshot: heavyMediaSnapshot
        )
    }

    static func assemble(
        logicalJobBudget: NativeClientResourceBudget,
        agentSnapshot: LocalAgentExecutionGovernorSnapshot,
        fileSnapshot: ClientFileWorkerGovernorSnapshot,
        transferSnapshot: ClientLargeTransferGovernorSnapshot,
        heavyMediaSnapshot: ClientHeavyMediaWorkGovernorSnapshot
    ) -> NativeClientMachineResourceSnapshot {
        .init(domains: [
            .init(
                domain: .swiftAgentRun,
                owner: .swiftAgentScheduler,
                admissionBoundary: .beforeDurableClaim,
                durableClaimStateWhileQueued: .unclaimed,
                observationLevel: .exact,
                processModel: .oneWorkerPerUnit,
                configuredLimit: agentSnapshot.limit,
                sharedPoolID: "background-logical-jobs",
                sharedPoolLimit: logicalJobBudget.totalLogicalJobs,
                activeCount: agentSnapshot.activeCount,
                queuedCount: agentSnapshot.queuedCount
            ),
            .init(
                domain: .localAgentHostWorkItem,
                owner: .rustHostCoordinator,
                admissionBoundary: .dedicatedWorkerBeforeDurableClaim,
                durableClaimStateWhileQueued: .unclaimed,
                observationLevel: .configuredLimitOnly,
                processModel: .hostWorkerPool,
                configuredLimit: logicalJobBudget.localAgentHostWorkItems,
                sharedPoolID: "background-logical-jobs",
                sharedPoolLimit: logicalJobBudget.totalLogicalJobs,
                activeCount: nil,
                queuedCount: nil
            ),
            Self.fileDomain(
                .mediaWorker,
                kind: .media,
                snapshot: fileSnapshot
            ),
            Self.fileDomain(
                .codeReadWorker,
                kind: .codeRead,
                snapshot: fileSnapshot
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
        ])
    }

    private static func fileDomain(
        _ domain: NativeClientResourceDomain,
        kind: ClientFileWorkerKind,
        snapshot: ClientFileWorkerGovernorSnapshot
    ) -> NativeClientResourceDomainSnapshot {
        .init(
            domain: domain,
            owner: .fileWorkerGovernor,
            admissionBoundary: .beforeProcessSpawn,
            durableClaimStateWhileQueued: .notApplicable,
            observationLevel: .exact,
            processModel: .oneWorkerPerUnit,
            configuredLimit: snapshot.budget.maximumConcurrentWorkers(for: kind),
            sharedPoolID: "file-workers",
            sharedPoolLimit: snapshot.budget.totalConcurrentWorkers,
            activeCount: snapshot.active.filter { $0.kind == kind }.count,
            queuedCount: snapshot.queued.filter { $0.kind == kind }.count
        )
    }
}
