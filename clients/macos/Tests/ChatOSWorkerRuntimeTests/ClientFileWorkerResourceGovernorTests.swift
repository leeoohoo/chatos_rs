import XCTest
@testable import ChatOSWorkerRuntime

final class ClientFileWorkerResourceGovernorTests: XCTestCase {
    func testAdaptiveBudgetUsesBothProcessorAndMemoryBounds() {
        let highCapacity = ClientFileWorkerResourceBudgetPolicy.recommended(
            logicalProcessorCount: 12,
            physicalMemoryBytes: 16 * 1_024 * 1_024 * 1_024
        )
        XCTAssertEqual(highCapacity.totalConcurrentWorkers, 10)
        XCTAssertEqual(highCapacity.maximumMediaWorkers, 4)
        XCTAssertEqual(highCapacity.maximumCodeReadWorkers, 8)

        let constrained = ClientFileWorkerResourceBudgetPolicy.recommended(
            logicalProcessorCount: 4,
            physicalMemoryBytes: 4 * 1_024 * 1_024 * 1_024
        )
        XCTAssertEqual(constrained.totalConcurrentWorkers, 3)
        XCTAssertEqual(constrained.maximumMediaWorkers, 2)
        XCTAssertEqual(constrained.maximumCodeReadWorkers, 2)

        let overridden = ClientFileWorkerResourceBudgetPolicy.recommended(
            logicalProcessorCount: 4,
            physicalMemoryBytes: 4 * 1_024 * 1_024 * 1_024,
            configuredTotalConcurrentWorkers: 6
        )
        XCTAssertEqual(overridden.totalConcurrentWorkers, 6)
    }

    func testCodeReadBurstCannotConsumeReservedMediaSlot() async throws {
        let governor = makeGovernor()
        let first = try await governor.acquire(context: context(.codeRead, "first"))
        let second = try await governor.acquire(context: context(.codeRead, "second"))
        let thirdContext = context(.codeRead, "third")
        let thirdTask = Task {
            try await governor.acquire(context: thirdContext)
        }
        try await waitUntil { await governor.snapshot().queued.count == 1 }

        let media = try await governor.acquire(context: context(.media, "preview"))
        var snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.active.count, 3)
        XCTAssertEqual(snapshot.active.filter { $0.kind == .media }.count, 1)
        XCTAssertEqual(snapshot.queued.map(\.operation), ["third"])

        await first.releaseAndWait()
        let third = try await thirdTask.value
        snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.active.count, 3)
        XCTAssertEqual(snapshot.metrics.maximumObservedActive, 3)
        XCTAssertEqual(snapshot.metrics.queuedAdmissionCount, 1)

        await second.releaseAndWait()
        await third.releaseAndWait()
        await media.releaseAndWait()
        snapshot = await governor.snapshot()
        XCTAssertTrue(snapshot.active.isEmpty)
    }

    func testQueuedCancellationRemovesOnlyCancelledRequest() async throws {
        let governor = makeGovernor()
        let first = try await governor.acquire(context: context(.media, "first"))
        let second = try await governor.acquire(context: context(.media, "second"))
        let cancelledContext = context(.media, "cancelled")
        let queued = Task {
            try await governor.acquire(context: cancelledContext)
        }
        try await waitUntil { await governor.snapshot().queued.count == 1 }
        queued.cancel()
        do {
            _ = try await queued.value
            XCTFail("Cancelled admission unexpectedly received a permit")
        } catch is CancellationError {}
        try await waitUntil { await governor.snapshot().queued.isEmpty }

        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.active.count, 2)
        XCTAssertEqual(snapshot.metrics.queuedCancellationCount, 1)
        await first.releaseAndWait()
        await second.releaseAndWait()
    }

    func testQueueLimitRejectsOnlyExcessRequest() async throws {
        let governor = ClientFileWorkerResourceGovernor(
            budget: .init(
                totalConcurrentWorkers: 3,
                maximumMediaWorkers: 2,
                maximumCodeReadWorkers: 2
            ),
            maximumQueued: 1
        )
        let first = try await governor.acquire(context: context(.codeRead, "first"))
        let second = try await governor.acquire(context: context(.codeRead, "second"))
        let queuedContext = context(.codeRead, "queued")
        let queued = Task {
            try await governor.acquire(context: queuedContext)
        }
        try await waitUntil { await governor.snapshot().queued.count == 1 }

        do {
            _ = try await governor.acquire(context: context(.codeRead, "overflow"))
            XCTFail("Queue overflow unexpectedly received a permit")
        } catch let error as ClientFileWorkerGovernorError {
            XCTAssertEqual(error, .queueFull)
        }

        queued.cancel()
        _ = try? await queued.value
        await first.releaseAndWait()
        await second.releaseAndWait()
        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.metrics.queueFullRejectionCount, 1)
    }

    private func makeGovernor() -> ClientFileWorkerResourceGovernor {
        .init(budget: .init(
            totalConcurrentWorkers: 3,
            maximumMediaWorkers: 2,
            maximumCodeReadWorkers: 2
        ))
    }

    private func context(
        _ kind: ClientFileWorkerKind,
        _ operation: String
    ) -> ClientFileWorkerContext {
        .init(kind: kind, priority: .interactive, operation: operation)
    }

    private func waitUntil(
        _ condition: @escaping @Sendable () async -> Bool
    ) async throws {
        for _ in 0..<100 {
            if await condition() { return }
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTFail("Timed out waiting for governor state")
    }
}
