import ChatOSConnector
import ChatOSWorkerRuntime
import Foundation

struct ClientMachineConcurrencyTargets: Equatable {
    let host: Int
    let app: Int
}

enum ClientMachineConcurrencyPolicy {
    static func targets(
        total: Int,
        baseHost: Int,
        hostActive: Int,
        hostQueued: Int,
        appUsedResourceUnits: Int,
        appQueuedResourceUnits: Int
    ) -> ClientMachineConcurrencyTargets {
        let base = ClientMachineConcurrencyTargets(host: baseHost, app: total - baseHost)
        if hostQueued > 0, appQueuedResourceUnits == 0 {
            let appKeep = max(1, (appUsedResourceUnits + 1) / 2)
            let hostDemand = max(base.host, hostActive + hostQueued)
            let host = min(total - appKeep, hostDemand)
            return .init(host: host, app: total - host)
        }
        if appQueuedResourceUnits > 0, hostQueued == 0 {
            let hostKeep = max(1, hostActive)
            let appDemand = max(base.app, (appUsedResourceUnits + appQueuedResourceUnits + 1) / 2)
            let app = min(total - hostKeep, appDemand)
            return .init(host: total - app, app: app)
        }
        return base
    }
}

actor ClientMachineConcurrencyAllocator {
    typealias HostSnapshot = @Sendable () async -> NativeLocalAgentHostResourceSnapshot?
    typealias HostTargetUpdate = @Sendable (Int) async -> Bool

    private let budget: NativeClientResourceBudget
    private let governor: ClientMachineResourceGovernor
    private let hostSnapshot: HostSnapshot
    private let hostTargetUpdate: HostTargetUpdate
    private let refreshInterval: Duration
    private var task: Task<Void, Never>?
    private var pendingBorrow: ClientMachineConcurrencyTargets?
    private var pendingBorrowObservations = 0

    init(
        budget: NativeClientResourceBudget,
        governor: ClientMachineResourceGovernor,
        refreshInterval: Duration = .seconds(1),
        hostSnapshot: @escaping HostSnapshot,
        hostTargetUpdate: @escaping HostTargetUpdate
    ) {
        self.budget = budget
        self.governor = governor
        self.refreshInterval = refreshInterval
        self.hostSnapshot = hostSnapshot
        self.hostTargetUpdate = hostTargetUpdate
    }

    func start() {
        guard budget.allowsDynamicBorrowing, task == nil else { return }
        task = Task { [weak self] in
            await self?.runLoop()
        }
    }

    func stop() {
        task?.cancel()
        task = nil
    }

    private func runLoop() async {
        while !Task.isCancelled {
            await rebalance()
            do {
                try await Task.sleep(for: refreshInterval)
            } catch {
                return
            }
        }
    }

    func rebalance() async {
        async let host = hostSnapshot()
        async let app = governor.snapshot()
        let appSnapshot = await app
        guard let hostSnapshot = await host else {
            pendingBorrow = nil
            pendingBorrowObservations = 0
            let baseAppUnits = budget.swiftAgentRuns
                * ClientMachineResourceBudget.agentRunUnitCost
            if appSnapshot.targetResourceUnits > baseAppUnits {
                _ = await governor.updateTargetResourceUnits(baseAppUnits)
            }
            return
        }
        let queuedUnits = appSnapshot.queued.reduce(into: 0) { total, context in
            total += appSnapshot.budget.unitCost(for: context.kind)
        }
        let desired = hostSnapshot.acceptingNewWork
            ? ClientMachineConcurrencyPolicy.targets(
                total: budget.totalLogicalJobs,
                baseHost: budget.localAgentHostWorkItems,
                hostActive: hostSnapshot.activeWorkItems,
                hostQueued: hostSnapshot.queuedReadyWorkItems,
                appUsedResourceUnits: appSnapshot.usedResourceUnits,
                appQueuedResourceUnits: queuedUnits
            )
            : baseTargets
        guard let stableDesired = stabilized(desired) else { return }
        await apply(
            stableDesired,
            host: hostSnapshot,
            app: appSnapshot
        )
    }

    private var baseTargets: ClientMachineConcurrencyTargets {
        .init(
            host: budget.localAgentHostWorkItems,
            app: budget.swiftAgentRuns
        )
    }

    private func stabilized(
        _ desired: ClientMachineConcurrencyTargets
    ) -> ClientMachineConcurrencyTargets? {
        guard desired != baseTargets else {
            pendingBorrow = nil
            pendingBorrowObservations = 0
            return desired
        }
        if pendingBorrow != desired {
            pendingBorrow = desired
            pendingBorrowObservations = 1
            return nil
        }
        pendingBorrowObservations += 1
        return pendingBorrowObservations >= 2 ? desired : nil
    }

    func apply(
        _ desired: ClientMachineConcurrencyTargets,
        host: NativeLocalAgentHostResourceSnapshot,
        app: ClientMachineResourceGovernorSnapshot
    ) async {
        var hostTarget = host.targetConcurrentWorkItems
        var appTarget = app.targetResourceUnits / ClientMachineResourceBudget.agentRunUnitCost
        var shrank = false

        if desired.host < hostTarget {
            guard await hostTargetUpdate(desired.host) else { return }
            hostTarget = desired.host
            shrank = true
        }
        if desired.app < appTarget {
            guard await governor.updateTargetResourceUnits(
                desired.app * ClientMachineResourceBudget.agentRunUnitCost
            ) else { return }
            appTarget = desired.app
            shrank = true
        }
        guard !shrank,
              host.activeWorkItems <= hostTarget,
              app.usedResourceUnits
                <= appTarget * ClientMachineResourceBudget.agentRunUnitCost else {
            return
        }

        if desired.host > hostTarget {
            guard desired.host + appTarget <= budget.totalLogicalJobs,
                  await hostTargetUpdate(desired.host) else { return }
            hostTarget = desired.host
        }
        if desired.app > appTarget {
            guard hostTarget + desired.app <= budget.totalLogicalJobs else { return }
            _ = await governor.updateTargetResourceUnits(
                desired.app * ClientMachineResourceBudget.agentRunUnitCost
            )
        }
    }
}
