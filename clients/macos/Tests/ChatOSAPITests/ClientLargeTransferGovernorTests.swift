import ChatOSNetworking
import Foundation
import XCTest

final class ClientLargeTransferGovernorTests: XCTestCase {
    func testMetricsTrackImmediateAndQueuedAdmissions() async throws {
        let governor = ClientLargeTransferGovernor(limit: 2, maximumQueued: 2)
        let first = try await governor.acquire(context: context("first", direction: .upload))
        let second = try await governor.acquire(context: context("second", direction: .download))
        let waitingContext = context("waiting", direction: .download)
        let waiting = Task {
            try await governor.acquire(context: waitingContext)
        }
        try await waitUntil(governor: governor) { $0.queued.count == 1 }

        let queued = await governor.snapshot()
        XCTAssertEqual(queued.totalInFlight, 3)
        XCTAssertEqual(queued.metrics.admittedCount, 2)
        XCTAssertEqual(queued.metrics.queuedAdmissionCount, 0)
        XCTAssertEqual(queued.metrics.maximumObservedActive, 2)
        XCTAssertEqual(queued.metrics.maximumObservedQueued, 1)

        await first.releaseAndWait()
        let admitted = try await waiting.value
        let running = await governor.snapshot()
        XCTAssertEqual(running.totalInFlight, 2)
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

    func testMetricsTrackQueuedCancellationAndQueueFullRejection() async throws {
        let governor = ClientLargeTransferGovernor(limit: 1, maximumQueued: 1)
        let first = try await governor.acquire(context: context("first", direction: .upload))
        let waitingContext = context("waiting", direction: .download)
        let waiting = Task {
            try await governor.acquire(context: waitingContext)
        }
        try await waitUntil(governor: governor) { $0.queued.count == 1 }

        do {
            _ = try await governor.acquire(context: context("overflow", direction: .upload))
            XCTFail("An over-budget transfer queue unexpectedly admitted work")
        } catch ClientLargeTransferGovernorError.queueFull {
            // Expected.
        }
        waiting.cancel()
        _ = try? await waiting.value

        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.totalInFlight, 1)
        XCTAssertEqual(snapshot.metrics.queuedCancellationCount, 1)
        XCTAssertEqual(snapshot.metrics.queueFullRejectionCount, 1)
        XCTAssertEqual(snapshot.metrics.maximumObservedQueued, 1)
        await first.releaseAndWait()
    }

    func testFIFOAdmissionAndCancellationDoNotLeakCapacity() async throws {
        let governor = ClientLargeTransferGovernor(limit: 1, maximumQueued: 3)
        let first = try await governor.acquire(context: context("first", direction: .upload))
        let cancelledContext = context("cancelled", direction: .download)
        let cancelled = Task {
            try await governor.acquire(context: cancelledContext)
        }
        try await waitUntil(governor: governor) { $0.queued.count == 1 }
        let nextContext = context("next", direction: .upload)
        let next = Task {
            try await governor.acquire(context: nextContext)
        }
        try await waitUntil(governor: governor) { $0.queued.count == 2 }
        let queuePeak = await governor.snapshot()
        XCTAssertEqual(queuePeak.metrics.maximumObservedQueued, 2)

        cancelled.cancel()
        do {
            _ = try await cancelled.value
            XCTFail("Cancelled transfer unexpectedly received a permit")
        } catch is CancellationError {
            // Expected.
        }
        try await waitUntil(governor: governor) {
            $0.queued.map(\.taskID) == ["next"]
        }

        await first.releaseAndWait()
        let admitted = try await next.value
        let running = await governor.snapshot()
        XCTAssertEqual(running.active.map(\.taskID), ["next"])
        XCTAssertTrue(running.queued.isEmpty)
        await admitted.releaseAndWait()
        let finished = await governor.snapshot()
        XCTAssertTrue(finished.active.isEmpty)
    }

    func testQueueBudgetFailsClosed() async throws {
        let governor = ClientLargeTransferGovernor(limit: 1, maximumQueued: 1)
        let first = try await governor.acquire(context: context("first", direction: .upload))
        let waitingContext = context("waiting", direction: .download)
        let waiting = Task {
            try await governor.acquire(context: waitingContext)
        }
        try await waitUntil(governor: governor) { $0.queued.count == 1 }

        do {
            _ = try await governor.acquire(context: context("overflow", direction: .upload))
            XCTFail("An over-budget transfer queue unexpectedly admitted work")
        } catch ClientLargeTransferGovernorError.queueFull {
            // Expected.
        }

        waiting.cancel()
        _ = try? await waiting.value
        await first.releaseAndWait()
        let snapshot = await governor.snapshot()
        XCTAssertTrue(snapshot.active.isEmpty)
        XCTAssertTrue(snapshot.queued.isEmpty)
    }

    private func context(
        _ taskID: String,
        direction: ClientLargeTransferDirection
    ) -> ClientLargeTransferContext {
        .init(taskID: taskID, ownerSessionID: "session", direction: direction)
    }

    private func waitUntil(
        governor: ClientLargeTransferGovernor,
        predicate: (ClientLargeTransferGovernorSnapshot) -> Bool
    ) async throws {
        for _ in 0..<200 {
            if predicate(await governor.snapshot()) { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTFail("Timed out waiting for transfer governor state")
    }
}
