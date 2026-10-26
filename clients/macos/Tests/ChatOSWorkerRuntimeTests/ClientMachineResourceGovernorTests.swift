import XCTest
@testable import ChatOSWorkerRuntime

final class ClientMachineResourceGovernorTests: XCTestCase {
    func testAgentAndBothFileKindsBorrowOneWeightedPoolWithoutExceedingIt() async throws {
        let governor = makeGovernor()
        let agent = try await governor.acquire(context: Self.context(.agentRun, "agent"))
        let media = try await governor.acquire(context: Self.context(.mediaWorker, "media"))
        let codeRead = try await governor.acquire(context: Self.context(.codeReadWorker, "code"))

        var snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.usedResourceUnits, 4)
        XCTAssertEqual(snapshot.active.count, 3)
        XCTAssertEqual(snapshot.budget.reservedHostResourceUnits, 2)

        let queuedTask = Task {
            try await governor.acquire(context: Self.context(.mediaWorker, "queued"))
        }
        try await waitUntil { await governor.snapshot().queued.count == 1 }
        snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.usedResourceUnits, snapshot.budget.availableResourceUnits)
        XCTAssertEqual(snapshot.queued.map(\.operation), ["queued"])

        await media.releaseAndWait()
        let queued = try await queuedTask.value
        snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.usedResourceUnits, 4)
        XCTAssertEqual(snapshot.metrics(for: .mediaWorker).queuedAdmissionCount, 1)

        await agent.releaseAndWait()
        await codeRead.releaseAndWait()
        await queued.releaseAndWait()
        snapshot = await governor.snapshot()
        XCTAssertTrue(snapshot.active.isEmpty)
    }

    func testSingleFileWorkerCanBorrowLastUnitBesideAgent() async throws {
        let governor = ClientMachineResourceGovernor(budget: .init(
            availableResourceUnits: 3,
            reservedHostResourceUnits: 2,
            maximumAgentRuns: 1,
            maximumMediaWorkers: 2,
            maximumCodeReadWorkers: 2
        ))
        let agent = try await governor.acquire(context: Self.context(.agentRun, "agent"))
        let file = try await governor.acquire(context: Self.context(.mediaWorker, "file"))

        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.usedResourceUnits, 3)
        XCTAssertEqual(snapshot.active.count, 2)

        await agent.releaseAndWait()
        await file.releaseAndWait()
    }

    func testCancellationRemovesOnlyItsWaiterAcrossKinds() async throws {
        let governor = makeGovernor()
        let first = try await governor.acquire(context: Self.context(.agentRun, "first"))
        let second = try await governor.acquire(context: Self.context(.agentRun, "second"))
        let cancelled = Task {
            try await governor.acquire(context: Self.context(.mediaWorker, "cancelled"))
        }
        let retained = Task {
            try await governor.acquire(context: Self.context(.codeReadWorker, "retained"))
        }
        try await waitUntil { await governor.snapshot().queued.count == 2 }

        cancelled.cancel()
        do {
            _ = try await cancelled.value
            XCTFail("Cancelled waiter unexpectedly received a permit")
        } catch is CancellationError {}
        try await waitUntil { await governor.snapshot().queued.count == 1 }
        var snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.queued.map(\.operation), ["retained"])

        await first.releaseAndWait()
        let admitted = try await retained.value
        XCTAssertEqual(admitted.context.operation, "retained")

        await second.releaseAndWait()
        await admitted.releaseAndWait()
        snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.metrics(for: .mediaWorker).queuedCancellationCount, 1)
        XCTAssertEqual(snapshot.metrics(for: .codeReadWorker).queuedCancellationCount, 0)
    }

    func testInteractiveBurstYieldsToBackgroundAcrossResourceKinds() async throws {
        let governor = ClientMachineResourceGovernor(budget: .init(
            availableResourceUnits: 1,
            reservedHostResourceUnits: 2,
            maximumAgentRuns: 0,
            maximumMediaWorkers: 1,
            maximumCodeReadWorkers: 1
        ))
        let active = try await governor.acquire(context: Self.context(.codeReadWorker, "active"))
        let order = AdmissionOrder()
        let background = Task {
            let permit = try await governor.acquire(context: Self.context(
                .mediaWorker,
                "background",
                priority: .background
            ))
            await order.append("background")
            await permit.releaseAndWait()
        }
        try await waitUntil { await governor.snapshot().queued.count == 1 }

        var interactive: [Task<Void, Error>] = []
        for index in 0..<5 {
            interactive.append(Task {
                let permit = try await governor.acquire(context: Self.context(
                    .codeReadWorker,
                    "interactive-\(index)"
                ))
                await order.append("interactive-\(index)")
                await permit.releaseAndWait()
            })
            let expectedQueued = index + 2
            try await waitUntil { await governor.snapshot().queued.count == expectedQueued }
        }

        await active.releaseAndWait()
        for task in interactive { try await task.value }
        try await background.value

        let values = await order.values()
        XCTAssertEqual(Array(values.prefix(4)), (0..<4).map { "interactive-\($0)" })
        XCTAssertEqual(values[4], "background")
        XCTAssertEqual(values[5], "interactive-4")
        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.metrics(for: .mediaWorker).backgroundFairnessAdmissionCount, 1)
    }

    func testPressurePolicyScalesCapacityAndPreservesInteractiveReserve() {
        let budget = Self.budget(availableResourceUnits: 8)

        let normal = ClientMachineResourcePressurePolicy.capacity(
            budget: budget,
            pressureState: .normal
        )
        let elevated = ClientMachineResourcePressurePolicy.capacity(
            budget: budget,
            pressureState: Self.pressure(thermal: .fair)
        )
        let serious = ClientMachineResourcePressurePolicy.capacity(
            budget: budget,
            pressureState: Self.pressure(memory: .warning)
        )
        let critical = ClientMachineResourcePressurePolicy.capacity(
            budget: budget,
            pressureState: Self.pressure(memory: .critical)
        )

        XCTAssertEqual(normal.effectiveResourceUnits, 8)
        XCTAssertEqual(elevated.effectiveResourceUnits, 6)
        XCTAssertEqual(serious.effectiveResourceUnits, 4)
        XCTAssertEqual(critical.effectiveResourceUnits, 2)
        XCTAssertEqual(elevated.maximumBackgroundResourceUnits, 4)
        XCTAssertEqual(serious.maximumBackgroundResourceUnits, 2)
        XCTAssertEqual(critical.maximumBackgroundResourceUnits, 0)
    }

    func testPowerAndBatterySignalsMapToElevatedPressure() {
        XCTAssertEqual(
            Self.pressure(lowPowerModeEnabled: true).level,
            .elevated
        )
        XCTAssertEqual(
            Self.pressure(onBatteryPower: true, batteryLevel: 0.2).level,
            .elevated
        )
        XCTAssertEqual(
            Self.pressure(onBatteryPower: true, batteryLevel: 0.21).level,
            .normal
        )
        XCTAssertEqual(
            Self.pressure(onBatteryPower: false, batteryLevel: 0.1).level,
            .normal
        )
        XCTAssertEqual(Self.pressure(thermal: .critical).level, .critical)
    }

    func testCriticalPressureImmediatelyBlocksNewWorkWithoutRevokingActivePermits() async throws {
        let governor = makeGovernor()
        let first = try await governor.acquire(context: Self.context(.agentRun, "first"))
        let second = try await governor.acquire(context: Self.context(.agentRun, "second"))

        await governor.updatePressureState(Self.pressure(memory: .critical))
        var snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.capacity.pressureState.level, .critical)
        XCTAssertEqual(snapshot.capacity.effectiveResourceUnits, 2)
        XCTAssertEqual(snapshot.usedResourceUnits, 4)
        XCTAssertEqual(snapshot.active.count, 2)
        let rejected = await governor.tryAcquire(
            context: Self.context(.mediaWorker, "rejected")
        )
        XCTAssertNil(rejected)

        await first.releaseAndWait()
        snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.active.map(\.operation), ["second"])
        let stillOverCapacity = await governor.tryAcquire(
            context: Self.context(.mediaWorker, "still-over-capacity")
        )
        XCTAssertNil(stillOverCapacity)

        await second.releaseAndWait()
        let admitted = await governor.tryAcquire(
            context: Self.context(.agentRun, "interactive-reserve")
        )
        XCTAssertNotNil(admitted)
        await admitted?.releaseAndWait()
    }

    func testPressureCapacityKeepsTwoUnitsForInteractiveWork() async throws {
        let governor = ClientMachineResourceGovernor(
            budget: Self.budget(availableResourceUnits: 6)
        )
        await governor.updatePressureState(Self.pressure(memory: .warning))

        let background = await governor.tryAcquire(context: Self.context(
            .mediaWorker,
            "background",
            priority: .background
        ))
        XCTAssertNotNil(background)
        let rejectedBackground = await governor.tryAcquire(context: Self.context(
            .agentRun,
            "background-cannot-borrow-reserve",
            priority: .background
        ))
        XCTAssertNil(rejectedBackground)
        let interactive = await governor.tryAcquire(
            context: Self.context(.agentRun, "interactive")
        )
        XCTAssertNotNil(interactive)

        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.capacity.effectiveResourceUnits, 3)
        XCTAssertEqual(snapshot.capacity.maximumBackgroundResourceUnits, 1)
        XCTAssertEqual(snapshot.usedResourceUnits, 3)

        await background?.releaseAndWait()
        await interactive?.releaseAndWait()
    }

    func testRecoveryWaitsForHysteresisBeforeExpandingCapacity() async throws {
        let governor = ClientMachineResourceGovernor(
            budget: makeGovernorBudget(),
            recoveryDelay: .milliseconds(60)
        )
        await governor.updatePressureState(Self.pressure(memory: .critical))
        await governor.updatePressureState(.normal)

        var snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.capacity.pressureState.level, .critical)
        XCTAssertEqual(snapshot.pendingRecoveryState, .normal)
        let rejectedBeforeRecovery = await governor.tryAcquire(context: Self.context(
            .mediaWorker,
            "background-before-recovery",
            priority: .background
        ))
        XCTAssertNil(rejectedBeforeRecovery)

        try await waitUntil {
            await governor.snapshot().capacity.pressureState.level == .normal
        }
        snapshot = await governor.snapshot()
        XCTAssertNil(snapshot.pendingRecoveryState)
        let admitted = await governor.tryAcquire(context: Self.context(
            .mediaWorker,
            "background-after-recovery",
            priority: .background
        ))
        XCTAssertNotNil(admitted)
        await admitted?.releaseAndWait()
    }

    func testRenewedPressureCancelsAnOlderRecovery() async throws {
        let governor = ClientMachineResourceGovernor(
            budget: makeGovernorBudget(),
            recoveryDelay: .milliseconds(80)
        )
        let critical = Self.pressure(memory: .critical)
        await governor.updatePressureState(critical)
        await governor.updatePressureState(.normal)
        try await Task.sleep(for: .milliseconds(30))
        await governor.updatePressureState(critical)
        try await Task.sleep(for: .milliseconds(80))

        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.capacity.pressureState.level, .critical)
        XCTAssertNil(snapshot.pendingRecoveryState)
        let rejectedAfterRenewedPressure = await governor.tryAcquire(context: Self.context(
            .mediaWorker,
            "background-remains-blocked",
            priority: .background
        ))
        XCTAssertNil(rejectedAfterRenewedPressure)
    }

    func testRecoveredCapacityStillAppliesInteractiveBurstFairness() async throws {
        let governor = ClientMachineResourceGovernor(
            budget: .init(
                availableResourceUnits: 1,
                reservedHostResourceUnits: 2,
                maximumAgentRuns: 0,
                maximumMediaWorkers: 1,
                maximumCodeReadWorkers: 1
            ),
            recoveryDelay: .milliseconds(20)
        )
        let active = try await governor.acquire(
            context: Self.context(.codeReadWorker, "active")
        )
        await governor.updatePressureState(Self.pressure(memory: .critical))
        let order = AdmissionOrder()
        let background = Task {
            let permit = try await governor.acquire(context: Self.context(
                .mediaWorker,
                "background",
                priority: .background
            ))
            await order.append("background")
            await permit.releaseAndWait()
        }
        try await waitUntil { await governor.snapshot().queued.count == 1 }
        var interactive: [Task<Void, Error>] = []
        for index in 0..<5 {
            interactive.append(Task {
                let permit = try await governor.acquire(context: Self.context(
                    .codeReadWorker,
                    "interactive-\(index)"
                ))
                await order.append("interactive-\(index)")
                await permit.releaseAndWait()
            })
            let expectedQueued = index + 2
            try await waitUntil { await governor.snapshot().queued.count == expectedQueued }
        }

        await governor.updatePressureState(.normal)
        try await waitUntil {
            await governor.snapshot().capacity.pressureState.level == .normal
        }
        await active.releaseAndWait()
        for task in interactive { try await task.value }
        try await background.value

        let values = await order.values()
        XCTAssertEqual(Array(values.prefix(4)), (0..<4).map { "interactive-\($0)" })
        XCTAssertEqual(values[4], "background")
        XCTAssertEqual(values[5], "interactive-4")
    }

    private func makeGovernor() -> ClientMachineResourceGovernor {
        .init(budget: makeGovernorBudget())
    }

    private func makeGovernorBudget() -> ClientMachineResourceBudget {
        .init(
            availableResourceUnits: 4,
            reservedHostResourceUnits: 2,
            maximumAgentRuns: 2,
            maximumMediaWorkers: 3,
            maximumCodeReadWorkers: 3
        )
    }

    private static func budget(availableResourceUnits: Int) -> ClientMachineResourceBudget {
        .init(
            availableResourceUnits: availableResourceUnits,
            reservedHostResourceUnits: 2,
            maximumAgentRuns: max(1, availableResourceUnits / 2),
            maximumMediaWorkers: availableResourceUnits,
            maximumCodeReadWorkers: availableResourceUnits
        )
    }

    private static func pressure(
        memory: ClientMachineMemoryPressure = .normal,
        thermal: ClientMachineThermalPressure = .nominal,
        lowPowerModeEnabled: Bool = false,
        onBatteryPower: Bool = false,
        batteryLevel: Double? = nil
    ) -> ClientMachinePressureState {
        .init(
            memory: memory,
            thermal: thermal,
            lowPowerModeEnabled: lowPowerModeEnabled,
            onBatteryPower: onBatteryPower,
            batteryLevel: batteryLevel
        )
    }

    private static func context(
        _ kind: ClientMachineResourceKind,
        _ operation: String,
        priority: ClientMachineResourcePriority = .interactive
    ) -> ClientMachineResourceContext {
        .init(kind: kind, priority: priority, operation: operation)
    }

    private func waitUntil(
        _ condition: @escaping @Sendable () async -> Bool
    ) async throws {
        for _ in 0..<200 {
            if await condition() { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTFail("Timed out waiting for machine resource governor state")
    }
}

private actor AdmissionOrder {
    private var recorded: [String] = []

    func append(_ value: String) { recorded.append(value) }
    func values() -> [String] { recorded }
}
