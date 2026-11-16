@testable import ChatOSApp
import ChatOSConnector
import ChatOSWorkerRuntime
import XCTest

final class ClientMachineConcurrencyPolicyTests: XCTestCase {
    func testBalancedDemandUsesHalfOfTenJobsPerSide() {
        XCTAssertEqual(
            targets(hostActive: 5, hostQueued: 1, appUsed: 10, appQueued: 2),
            .init(host: 5, app: 5)
        )
    }

    func testHostBorrowsOnlyIdleAppSlotsUpToNine() {
        XCTAssertEqual(
            targets(hostActive: 5, hostQueued: 4, appUsed: 0, appQueued: 0),
            .init(host: 9, app: 1)
        )
        XCTAssertEqual(
            targets(hostActive: 5, hostQueued: 4, appUsed: 6, appQueued: 0),
            .init(host: 7, app: 3)
        )
    }

    func testAppBorrowsOnlyIdleHostSlotsUpToNine() {
        XCTAssertEqual(
            targets(hostActive: 0, hostQueued: 0, appUsed: 10, appQueued: 8),
            .init(host: 1, app: 9)
        )
        XCTAssertEqual(
            targets(hostActive: 3, hostQueued: 0, appUsed: 10, appQueued: 8),
            .init(host: 3, app: 7)
        )
    }

    func testEveryPlanPreservesOneSlotAndNeverExceedsTotal() {
        for hostActive in 0...9 {
            for hostQueued in 0...9 {
                for appUsed in 0...18 {
                    let result = targets(
                        hostActive: hostActive,
                        hostQueued: hostQueued,
                        appUsed: appUsed,
                        appQueued: hostQueued == 0 ? 4 : 0
                    )
                    XCTAssertGreaterThanOrEqual(result.host, 1)
                    XCTAssertGreaterThanOrEqual(result.app, 1)
                    XCTAssertEqual(result.host + result.app, 10)
                }
            }
        }
    }

    func testAllocatorShrinksAndDrainsBeforeGrowingTheBorrower() async {
        let budget = NativeClientResourceBudget(
            totalLogicalJobs: 10,
            localAgentHostWorkItems: 5
        )
        let governor = ClientMachineResourceGovernor(budget: .init(
            availableResourceUnits: 2,
            maximumAvailableResourceUnits: 18,
            reservedHostResourceUnits: 10,
            maximumAgentRuns: 9,
            maximumMediaWorkers: 9,
            maximumCodeReadWorkers: 9
        ))
        let recorder = HostTargetRecorder()
        let allocator = ClientMachineConcurrencyAllocator(
            budget: budget,
            governor: governor,
            hostSnapshot: { nil },
            hostTargetUpdate: { target in
                await recorder.record(target)
                return true
            }
        )
        let base = ClientMachineConcurrencyTargets(host: 5, app: 5)

        await allocator.apply(base, host: host(target: 9, active: 7), app: await governor.snapshot())
        var recorded = await recorder.values()
        var appSnapshot = await governor.snapshot()
        XCTAssertEqual(recorded, [5])
        XCTAssertEqual(appSnapshot.targetResourceUnits, 2)

        await allocator.apply(base, host: host(target: 5, active: 6), app: await governor.snapshot())
        appSnapshot = await governor.snapshot()
        XCTAssertEqual(appSnapshot.targetResourceUnits, 2)

        await allocator.apply(base, host: host(target: 5, active: 5), app: await governor.snapshot())
        appSnapshot = await governor.snapshot()
        recorded = await recorder.values()
        XCTAssertEqual(recorded, [5])
        XCTAssertEqual(appSnapshot.targetResourceUnits, 10)
    }

    func testMissingHostSnapshotNeverExpandsAppCapacity() async {
        let governor = ClientMachineResourceGovernor(budget: .init(
            availableResourceUnits: 2,
            maximumAvailableResourceUnits: 18,
            reservedHostResourceUnits: 10,
            maximumAgentRuns: 9,
            maximumMediaWorkers: 9,
            maximumCodeReadWorkers: 9
        ))
        let allocator = ClientMachineConcurrencyAllocator(
            budget: .init(totalLogicalJobs: 10, localAgentHostWorkItems: 5),
            governor: governor,
            hostSnapshot: { nil },
            hostTargetUpdate: { _ in false }
        )

        await allocator.rebalance()

        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.targetResourceUnits, 2)
    }

    private func targets(
        hostActive: Int,
        hostQueued: Int,
        appUsed: Int,
        appQueued: Int
    ) -> ClientMachineConcurrencyTargets {
        ClientMachineConcurrencyPolicy.targets(
            total: 10,
            baseHost: 5,
            hostActive: hostActive,
            hostQueued: hostQueued,
            appUsedResourceUnits: appUsed,
            appQueuedResourceUnits: appQueued
        )
    }

    private func host(target: Int, active: Int) -> NativeLocalAgentHostResourceSnapshot {
        .init(
            maximumConcurrentWorkItems: 9,
            targetConcurrentWorkItems: target,
            effectiveMaximumConcurrentWorkItems: target,
            pressureLevel: .normal,
            activeWorkItems: active,
            idleWorkers: 9 - active,
            queuedReadyWorkItems: 0
        )
    }
}

private actor HostTargetRecorder {
    private var targets: [Int] = []

    func record(_ target: Int) { targets.append(target) }
    func values() -> [Int] { targets }
}
