import ChatOSNetworking
import XCTest

final class ClientHeavyMediaWorkGovernorTests: XCTestCase {
    func testMetricsTrackImmediateAndQueuedAdmissions() async throws {
        let governor = ClientHeavyMediaWorkGovernor(limit: 2, maximumQueued: 2)
        let first = try await governor.acquire(context: context("first", kind: .imageDecode))
        let second = try await governor.acquire(context: context(
            "second",
            kind: .videoFrameExtraction
        ))
        let waitingContext = context("waiting", kind: .videoFrameExtraction)
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

    func testMetricsTrackAllRejectionAndCancellationReasons() async throws {
        let governor = ClientHeavyMediaWorkGovernor(
            limit: 1,
            maximumQueued: 1,
            maximumEstimatedDecodedBytes: 100
        )

        do {
            _ = try await governor.acquire(context: .init(
                taskID: "invalid",
                kind: .imageDecode,
                estimatedDecodedBytes: 0
            ))
            XCTFail("An invalid estimate unexpectedly received a permit")
        } catch ClientHeavyMediaWorkGovernorError.invalidEstimatedDecodedBytes {
            // Expected.
        }
        do {
            _ = try await governor.acquire(context: .init(
                taskID: "oversized",
                kind: .imageDecode,
                estimatedDecodedBytes: 101
            ))
            XCTFail("An oversized estimate unexpectedly received a permit")
        } catch ClientHeavyMediaWorkGovernorError.estimatedDecodedBytesExceedLimit {
            // Expected.
        }

        let first = try await governor.acquire(context: .init(
            taskID: "first",
            kind: .imageDecode,
            estimatedDecodedBytes: 50
        ))
        let waiting = Task {
            try await governor.acquire(context: .init(
                taskID: "waiting",
                kind: .videoFrameExtraction,
                estimatedDecodedBytes: 50
            ))
        }
        try await waitUntil(governor: governor) { $0.queued.count == 1 }
        do {
            _ = try await governor.acquire(context: .init(
                taskID: "overflow",
                kind: .imageDecode,
                estimatedDecodedBytes: 50
            ))
            XCTFail("An over-budget heavy-media queue unexpectedly admitted work")
        } catch ClientHeavyMediaWorkGovernorError.queueFull {
            // Expected.
        }
        waiting.cancel()
        _ = try? await waiting.value

        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.totalInFlight, 1)
        XCTAssertEqual(snapshot.metrics.invalidEstimateRejectionCount, 1)
        XCTAssertEqual(snapshot.metrics.overBudgetRejectionCount, 1)
        XCTAssertEqual(snapshot.metrics.queueFullRejectionCount, 1)
        XCTAssertEqual(snapshot.metrics.queuedCancellationCount, 1)
        XCTAssertEqual(snapshot.metrics.maximumObservedQueued, 1)
        await first.releaseAndWait()
    }

    func testFIFOAdmissionAndCancellationDoNotLeakHeavyMediaCapacity() async throws {
        let governor = ClientHeavyMediaWorkGovernor(limit: 1, maximumQueued: 3)
        let first = try await governor.acquire(context: context(
            "first",
            kind: .imageDecode
        ))
        let cancelledContext = context("cancelled", kind: .videoFrameExtraction)
        let cancelled = Task {
            try await governor.acquire(context: cancelledContext)
        }
        try await waitUntil(governor: governor) { $0.queued.count == 1 }
        let nextContext = context("next", kind: .imageDecode)
        let next = Task {
            try await governor.acquire(context: nextContext)
        }
        try await waitUntil(governor: governor) { $0.queued.count == 2 }
        let queuePeak = await governor.snapshot()
        XCTAssertEqual(queuePeak.metrics.maximumObservedQueued, 2)

        cancelled.cancel()
        do {
            _ = try await cancelled.value
            XCTFail("Cancelled heavy-media work unexpectedly received a permit")
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

    func testPerformReleasesCapacityAfterFailureAndQueueBudgetFailsClosed() async throws {
        enum ExpectedFailure: Error { case failed }

        let governor = ClientHeavyMediaWorkGovernor(limit: 1, maximumQueued: 1)
        do {
            _ = try await governor.perform(context: context(
                "failing",
                kind: .imageDecode
            )) { () async throws -> Int in
                throw ExpectedFailure.failed
            }
            XCTFail("Failing heavy-media operation unexpectedly succeeded")
        } catch ExpectedFailure.failed {
            // Expected.
        }
        let afterFailure = await governor.snapshot()
        XCTAssertTrue(afterFailure.active.isEmpty)
        XCTAssertEqual(afterFailure.maximumEstimatedDecodedBytes, 256 * 1_024 * 1_024)
        do {
            _ = try await governor.acquire(context: .init(
                taskID: "oversized",
                kind: .imageDecode,
                estimatedDecodedBytes: 256 * 1_024 * 1_024 + 1
            ))
            XCTFail("An over-budget decoded working set unexpectedly received a permit")
        } catch ClientHeavyMediaWorkGovernorError.estimatedDecodedBytesExceedLimit(
            let maximumBytes
        ) {
            XCTAssertEqual(maximumBytes, 256 * 1_024 * 1_024)
        }

        let first = try await governor.acquire(context: context(
            "first",
            kind: .imageDecode
        ))
        let waitingContext = context("waiting", kind: .videoFrameExtraction)
        let waiting = Task {
            try await governor.acquire(context: waitingContext)
        }
        try await waitUntil(governor: governor) { $0.queued.count == 1 }
        do {
            _ = try await governor.acquire(context: context(
                "overflow",
                kind: .imageDecode
            ))
            XCTFail("An over-budget heavy-media queue unexpectedly admitted work")
        } catch ClientHeavyMediaWorkGovernorError.queueFull {
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
        kind: ClientHeavyMediaWorkKind
    ) -> ClientHeavyMediaWorkContext {
        .init(
            taskID: taskID,
            ownerSessionID: "session",
            kind: kind,
            estimatedDecodedBytes: 64 * 1_024 * 1_024
        )
    }

    private func waitUntil(
        governor: ClientHeavyMediaWorkGovernor,
        predicate: (ClientHeavyMediaWorkGovernorSnapshot) -> Bool
    ) async throws {
        for _ in 0..<200 {
            if predicate(await governor.snapshot()) { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTFail("Timed out waiting for heavy-media governor state")
    }
}
