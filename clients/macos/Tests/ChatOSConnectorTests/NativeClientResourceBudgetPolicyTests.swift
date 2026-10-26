@testable import ChatOSConnector
import ChatOSWorkerRuntime
import XCTest

final class NativeClientResourceBudgetPolicyTests: XCTestCase {
    func testMachineBudgetMatchesNativeHostReservationAndSwiftAgentCapacity() {
        let native = NativeClientResourceBudgetPolicy.recommended(
            logicalProcessorCount: 12,
            physicalMemoryBytes: 96 * 1_024 * 1_024 * 1_024,
            configuredTotalLogicalJobs: 10,
            configuredHostWorkItems: 2
        )
        let machine = ClientMachineResourceBudgetPolicy.recommended(
            logicalProcessorCount: 12,
            physicalMemoryBytes: 96 * 1_024 * 1_024 * 1_024,
            configuredTotalLogicalJobs: 10,
            configuredHostWorkItems: 2,
            fileBudget: .init(
                totalConcurrentWorkers: 6,
                maximumMediaWorkers: 4,
                maximumCodeReadWorkers: 5
            )
        )

        XCTAssertEqual(machine.maximumAgentRuns, native.swiftAgentRuns)
        XCTAssertEqual(
            machine.reservedHostResourceUnits,
            native.localAgentHostWorkItems * ClientMachineResourceBudget.agentRunUnitCost
        )
        XCTAssertEqual(
            machine.availableResourceUnits,
            native.swiftAgentRuns * ClientMachineResourceBudget.agentRunUnitCost
        )
    }

    func testAdaptiveBudgetSplitsOneTotalAcrossHostAndSwiftAgent() {
        let budget = NativeClientResourceBudgetPolicy.recommended(
            logicalProcessorCount: 12,
            physicalMemoryBytes: 96 * 1_024 * 1_024 * 1_024
        )

        XCTAssertEqual(budget.totalLogicalJobs, 10)
        XCTAssertEqual(budget.localAgentHostWorkItems, 2)
        XCTAssertEqual(budget.swiftAgentRuns, 8)
        XCTAssertEqual(
            budget.localAgentHostWorkItems + budget.swiftAgentRuns,
            budget.totalLogicalJobs
        )
    }

    func testConfiguredHostCannotConsumeSwiftAgentReserve() {
        let budget = NativeClientResourceBudgetPolicy.recommended(
            logicalProcessorCount: 12,
            physicalMemoryBytes: 96 * 1_024 * 1_024 * 1_024,
            configuredTotalLogicalJobs: 5,
            configuredHostWorkItems: 16
        )

        XCTAssertEqual(budget.totalLogicalJobs, 5)
        XCTAssertEqual(budget.localAgentHostWorkItems, 4)
        XCTAssertEqual(budget.swiftAgentRuns, 1)
    }

    func testInvalidConfigurationFallsBackToAdaptiveBounds() {
        XCTAssertNil(NativeClientResourceBudgetPolicy.validatedTotal(1))
        XCTAssertNil(NativeClientResourceBudgetPolicy.validatedTotal(17))
        XCTAssertEqual(
            NativeClientResourceBudgetPolicy.recommended(
                logicalProcessorCount: 2,
                physicalMemoryBytes: 4 * 1_024 * 1_024 * 1_024,
                configuredTotalLogicalJobs: 100
            ).totalLogicalJobs,
            2
        )
        XCTAssertEqual(
            NativeClientResourceBudgetPolicy.recommended(
                logicalProcessorCount: 128,
                physicalMemoryBytes: 4 * 1_024 * 1_024 * 1_024
            ).totalLogicalJobs,
            2
        )
    }

    func testAdaptiveBudgetRequiresBothProcessorAndMemoryCapacity() {
        let memoryBound = NativeClientResourceBudgetPolicy.recommended(
            logicalProcessorCount: 12,
            physicalMemoryBytes: 8 * 1_024 * 1_024 * 1_024
        )
        XCTAssertEqual(memoryBound.totalLogicalJobs, 3)
        XCTAssertEqual(memoryBound.localAgentHostWorkItems, 1)
        XCTAssertEqual(memoryBound.swiftAgentRuns, 2)

        let processorBound = NativeClientResourceBudgetPolicy.recommended(
            logicalProcessorCount: 8,
            physicalMemoryBytes: 96 * 1_024 * 1_024 * 1_024
        )
        XCTAssertEqual(processorBound.totalLogicalJobs, 6)
        XCTAssertEqual(processorBound.localAgentHostWorkItems, 1)
        XCTAssertEqual(processorBound.swiftAgentRuns, 5)
    }

    func testExecutionGovernorCancellationReturnsQueueCapacity() async throws {
        let governor = LocalAgentExecutionGovernor(limit: 1)
        let firstValue = await governor.acquire()
        let first = try XCTUnwrap(firstValue)
        let waiting = Task { await governor.acquire() }
        try await waitUntil(governor: governor) { $0.queuedCount == 1 }

        waiting.cancel()
        let cancelledPermit = await waiting.value
        XCTAssertNil(cancelledPermit)
        let afterCancellation = await governor.snapshot()
        XCTAssertEqual(afterCancellation.metrics.queuedCancellationCount, 1)
        XCTAssertEqual(afterCancellation.totalInFlight, 1)

        await first.releaseAndWait()
        let replacementValue = await governor.acquire()
        let replacement = try XCTUnwrap(replacementValue)
        let overBudget = await governor.tryAcquire()
        XCTAssertNil(overBudget)
        await replacement.releaseAndWait()
    }

    func testExecutionGovernorSnapshotTracksImmediateAndQueuedAdmissions() async throws {
        let governor = LocalAgentExecutionGovernor(limit: 2, maximumQueued: 2)
        let firstValue = await governor.acquire()
        let first = try XCTUnwrap(firstValue)
        let secondValue = await governor.acquire()
        let second = try XCTUnwrap(secondValue)
        let waiting = Task { await governor.acquire() }
        try await waitUntil(governor: governor) { $0.queuedCount == 1 }

        let queued = await governor.snapshot()
        XCTAssertEqual(queued.limit, 2)
        XCTAssertEqual(queued.maximumQueued, 2)
        XCTAssertEqual(queued.activeCount, 2)
        XCTAssertEqual(queued.totalInFlight, 3)
        XCTAssertEqual(queued.metrics.admittedCount, 2)
        XCTAssertEqual(queued.metrics.maximumObservedActive, 2)
        XCTAssertEqual(queued.metrics.maximumObservedQueued, 1)

        await first.releaseAndWait()
        let admittedValue = await waiting.value
        let admitted = try XCTUnwrap(admittedValue)
        let running = await governor.snapshot()
        XCTAssertEqual(running.activeCount, 2)
        XCTAssertEqual(running.queuedCount, 0)
        XCTAssertEqual(running.metrics.admittedCount, 3)
        XCTAssertEqual(running.metrics.queuedAdmissionCount, 1)
        XCTAssertEqual(running.metrics.releasedCount, 1)
        XCTAssertGreaterThan(running.metrics.totalQueueWaitNanoseconds, 0)
        XCTAssertEqual(
            running.metrics.maximumQueueWaitNanoseconds,
            running.metrics.totalQueueWaitNanoseconds
        )

        await second.releaseAndWait()
        await admitted.releaseAndWait()
        await admitted.releaseAndWait()
        let finished = await governor.snapshot()
        XCTAssertEqual(finished.totalInFlight, 0)
        XCTAssertEqual(finished.metrics.releasedCount, 3)
    }

    func testExecutionGovernorQueueFailsClosedAndTracksCapacityRejections() async throws {
        let governor = LocalAgentExecutionGovernor(limit: 1, maximumQueued: 1)
        let firstValue = await governor.acquire()
        let first = try XCTUnwrap(firstValue)
        let waiting = Task { await governor.acquire() }
        try await waitUntil(governor: governor) { $0.queuedCount == 1 }

        let queueOverflow = await governor.acquire()
        XCTAssertNil(queueOverflow)
        let immediateRejection = await governor.tryAcquire()
        XCTAssertNil(immediateRejection)
        let rejected = await governor.snapshot()
        XCTAssertEqual(rejected.totalInFlight, 2)
        XCTAssertEqual(rejected.metrics.queueFullRejectionCount, 1)
        XCTAssertEqual(rejected.metrics.immediateCapacityRejectionCount, 1)

        waiting.cancel()
        let cancelled = await waiting.value
        XCTAssertNil(cancelled)
        await first.releaseAndWait()
        let finished = await governor.snapshot()
        XCTAssertEqual(finished.totalInFlight, 0)
        XCTAssertEqual(finished.metrics.queuedCancellationCount, 1)
        XCTAssertEqual(finished.metrics.releasedCount, 1)
    }

    func testExecutionGovernorPrioritizesInteractiveWorkOverOlderBackgroundWaiter() async throws {
        let governor = LocalAgentExecutionGovernor(limit: 1)
        let activeValue = await governor.acquire()
        let active = try XCTUnwrap(activeValue)
        let background = Task { await governor.acquire(priority: .background) }
        try await waitUntil(governor: governor) { $0.backgroundQueuedCount == 1 }
        let interactive = Task { await governor.acquire(priority: .interactive) }
        try await waitUntil(governor: governor) {
            $0.backgroundQueuedCount == 1 && $0.interactiveQueuedCount == 1
        }

        await active.releaseAndWait()
        let interactiveValue = await interactive.value
        let interactivePermit = try XCTUnwrap(interactiveValue)
        let prioritized = await governor.snapshot()
        XCTAssertEqual(prioritized.backgroundQueuedCount, 1)
        XCTAssertEqual(prioritized.interactiveQueuedCount, 0)
        XCTAssertEqual(prioritized.metrics.interactivePriorityBypassCount, 1)

        await interactivePermit.releaseAndWait()
        let backgroundValue = await background.value
        let backgroundPermit = try XCTUnwrap(backgroundValue)
        await backgroundPermit.releaseAndWait()
    }

    func testExecutionGovernorBoundsInteractiveBurstSoBackgroundCannotStarve() async throws {
        let governor = LocalAgentExecutionGovernor(limit: 1)
        let activeValue = await governor.acquire()
        let active = try XCTUnwrap(activeValue)
        let order = LocalAgentExecutionAdmissionOrder()
        let background = Task {
            let permit = await governor.acquire(priority: .background)
            await order.append("background")
            await permit?.releaseAndWait()
        }
        try await waitUntil(governor: governor) { $0.backgroundQueuedCount == 1 }
        var interactiveTasks: [Task<Void, Never>] = []
        for index in 0..<5 {
            interactiveTasks.append(Task {
                let permit = await governor.acquire(priority: .interactive)
                await order.append("interactive-\(index)")
                await permit?.releaseAndWait()
            })
            try await waitUntil(governor: governor) { $0.interactiveQueuedCount == index + 1 }
        }

        await active.releaseAndWait()
        await background.value
        for task in interactiveTasks { await task.value }

        let values = await order.values()
        XCTAssertEqual(Array(values.prefix(4)), (0..<4).map { "interactive-\($0)" })
        XCTAssertEqual(values[4], "background")
        XCTAssertEqual(values[5], "interactive-4")
        let finished = await governor.snapshot()
        XCTAssertEqual(finished.metrics.backgroundFairnessAdmissionCount, 1)
        XCTAssertEqual(finished.totalInFlight, 0)
    }

    private func waitUntil(
        governor: LocalAgentExecutionGovernor,
        predicate: (LocalAgentExecutionGovernorSnapshot) -> Bool
    ) async throws {
        for _ in 0..<200 {
            if predicate(await governor.snapshot()) { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTFail("Timed out waiting for execution governor state")
    }
}

private actor LocalAgentExecutionAdmissionOrder {
    private var recorded: [String] = []

    func append(_ value: String) { recorded.append(value) }
    func values() -> [String] { recorded }
}
