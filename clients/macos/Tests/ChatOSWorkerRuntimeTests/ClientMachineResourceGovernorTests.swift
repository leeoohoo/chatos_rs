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
        }
        try await waitUntil { await governor.snapshot().queued.count == 6 }

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

    private func makeGovernor() -> ClientMachineResourceGovernor {
        .init(budget: .init(
            availableResourceUnits: 4,
            reservedHostResourceUnits: 2,
            maximumAgentRuns: 2,
            maximumMediaWorkers: 3,
            maximumCodeReadWorkers: 3
        ))
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
